#!/usr/bin/env python3
"""Build a Skyward recovery image from a stock one, without the Unix-only
skyward-pack host tool.

The same steps `skyward-pack recovery` takes: the stock image's kernel and
ramdisk, the recovery service replaced by the built binary, the commands
installs look for linked to it, the fonts, configuration and first program
of a second system added, the build properties its secure services read
left for the recovery to fill in from the installed system, the ramdisk
packed again as an LZ4 legacy stream and the whole thing ending in an
unsigned AVB footer, laid out as `add_hash_footer --algorithm NONE` does.
"""

import argparse
import hashlib
import os
from pathlib import Path
import struct
import sys

import lz4.block
import lz4.frame

PAGE = 4096
LZ4_LEGACY_MAGIC = 0x184C2102
LZ4_LEGACY_BLOCK = 8 << 20

S_IFMT = 0o170000
S_IFDIR = 0o040000
S_IFREG = 0o100000
S_IFLNK = 0o120000

COMMANDS = ["7za", "avbctl", "resetprop", "unzip", "zstd"]
BUILD_VERSION = [
    "ro.build.version.release",
    "ro.build.version.security_patch",
    "ro.vendor.build.security_patch",
]


class Entry:
    def __init__(self, name, mode, uid, gid, mtime, nlink, dev, rdev, data):
        self.name = name
        self.mode = mode
        self.uid = uid
        self.gid = gid
        self.mtime = mtime
        self.nlink = nlink
        self.dev = dev
        self.rdev = rdev
        self.data = data


def align(n, to=PAGE):
    return (n + to - 1) // to * to


def parse_newc(buf):
    entries = []
    at = 0
    while True:
        if buf[at : at + 6] != b"070701":
            raise SystemExit(f"not a newc archive at {at}")
        fields = [int(buf[at + 6 + 8 * i : at + 14 + 8 * i], 16) for i in range(13)]
        ino, mode, uid, gid, nlink, mtime, size = fields[:7]
        devmajor, devminor, rdevmajor, rdevminor, namesize = fields[7:12]
        name_at = at + 110
        name = buf[name_at : name_at + namesize - 1].decode()
        data_at = align(name_at + namesize, 4)
        data = buf[data_at : data_at + size]
        at = align(data_at + size, 4)
        if name == "TRAILER!!!":
            break
        entries.append(Entry(name, mode, uid, gid, mtime, nlink, (devmajor, devminor), (rdevmajor, rdevminor), data))
    return entries


def build_newc(entries):
    out = bytearray()
    for i, e in enumerate(entries):
        hdr = "070701" + "".join(
            f"{v:08x}"
            for v in (
                300_000 + i,
                e.mode,
                e.uid,
                e.gid,
                e.nlink,
                e.mtime,
                len(e.data),
                e.dev[0],
                e.dev[1],
                e.rdev[0],
                e.rdev[1],
                len(e.name.encode()) + 1,
                0,
            )
        )
        out += hdr.encode()
        out += e.name.encode() + b"\0"
        out += b"\0" * (align(len(out), 4) - len(out))
        out += e.data
        out += b"\0" * (align(len(out), 4) - len(out))
    trailer = "070701" + "".join(f"{v:08x}" for v in (0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 11, 0))
    out += trailer.encode() + b"TRAILER!!!\0"
    out += b"\0" * (align(len(out), 4) - len(out))
    return bytes(out)


def decode_ramdisk(compressed):
    if compressed[:4] == struct.pack("<I", LZ4_LEGACY_MAGIC):
        out = bytearray()
        at = 4
        while at + 4 <= len(compressed):
            n = struct.unpack_from("<I", compressed, at)[0]
            at += 4
            if n == 0 or at + n > len(compressed):
                break
            block = compressed[at : at + n]
            at += n
            try:
                out += lz4.block.decompress(block, uncompressed_size=LZ4_LEGACY_BLOCK)
            except Exception:
                out += lz4.block.decompress(block)
        cpio = bytes(out)
    else:
        cpio = lz4.frame.decompress(compressed)
    return parse_newc(cpio)


def encode_ramdisk(entries):
    cpio = build_newc(entries)
    out = bytearray(struct.pack("<I", LZ4_LEGACY_MAGIC))
    for at in range(0, len(cpio), LZ4_LEGACY_BLOCK):
        block = lz4.block.compress(
            cpio[at : at + LZ4_LEGACY_BLOCK], mode="high_compression", compression=12, store_size=False
        )
        out += struct.pack("<I", len(block)) + block
    return bytes(out)


def find(entries, name):
    for e in entries:
        if e.name == name:
            return e
    return None


def insert(entries, name, mode, data):
    if find(entries, name) is not None:
        raise SystemExit(f"{name} is already in the ramdisk")
    parts = name.split("/")
    parent = ""
    for part in parts[:-1]:
        parent = f"{parent}/{part}" if parent else part
        existing = find(entries, parent)
        if existing is None:
            entries.append(Entry(parent, S_IFDIR | 0o755, 0, 0, 0, 2, (0, 0), (0, 0), b""))
        elif existing.mode & S_IFMT != S_IFDIR:
            raise SystemExit(f"{parent} is not a directory")
    entries.append(Entry(name, mode, 0, 0, 0, 2 if mode & S_IFMT == S_IFDIR else 1, (0, 0), (0, 0), data))


def link(entries, name, target):
    existing = find(entries, name)
    if existing is None:
        insert(entries, name, S_IFLNK | 0o755, target.encode())
    elif existing.mode & S_IFMT == S_IFLNK:
        existing.data = target.encode()
    else:
        raise SystemExit(f"{name} is in the ramdisk and is not a link")


def replace(entries, name, data):
    e = find(entries, name)
    if e is None:
        raise SystemExit(f"{name} is not in the ramdisk")
    if e.mode & S_IFMT != S_IFREG:
        raise SystemExit(f"{name} is not a regular file")
    e.data = data


def without(props, names):
    out = bytearray()
    for line in props.splitlines(keepends=True):
        key = line.split(b"=")[0]
        if key.decode(errors="ignore") not in names:
            out += line
    return bytes(out)


def hash_descriptor(image_size, partition, salt, digest):
    algorithm = b"sha256" + b"\0" * 26
    d = bytearray()
    d += struct.pack(">QQ", 2, 0)
    d += struct.pack(">Q", image_size)
    d += algorithm
    for n in (len(partition), len(salt), len(digest), 0):
        d += struct.pack(">I", n)
    d += b"\0" * 60
    d += partition.encode() + salt + digest
    d += b"\0" * (align(len(d), 8) - len(d))
    struct.pack_into(">Q", d, 8, len(d) - 16)
    return bytes(d)


def vbmeta(descriptors):
    aux = align(len(descriptors), 64)
    v = bytearray(b"AVB0")
    v += struct.pack(">II", 1, 0)
    v += struct.pack(">QQ", 0, aux)
    v += struct.pack(">I", 0)
    n = len(descriptors)
    for field in (0, 0, 0, 0, n, 0, n, 0, 0, n, 0):
        v += struct.pack(">Q", field)
    v += struct.pack(">II", 0, 0)
    v += b"skyward-pack"
    v += b"\0" * (256 - len(v))
    v += descriptors
    v += b"\0" * (aux - len(descriptors))
    return bytes(v)


def footed(image, partition, partition_size, salt=None):
    if salt is None:
        salt = hashlib.sha256(image).digest()
    digest = hashlib.sha256(salt + image).digest()
    vb = vbmeta(hash_descriptor(len(image), partition, salt, digest))
    offset = align(len(image))
    end = offset + align(len(vb))
    if end + PAGE > partition_size:
        raise SystemExit(f"a {len(image)}-byte image and its vbmeta image do not fit {partition_size} bytes")
    out = bytearray(image)
    out += b"\0" * (offset - len(out))
    out += vb
    out += b"\0" * (partition_size - 64 - len(out))
    out += b"AVBf" + struct.pack(">II", 1, 0)
    for n in (len(image), offset, len(vb)):
        out += struct.pack(">Q", n)
    out += b"\0" * (partition_size - len(out))
    return bytes(out)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stock", required=True)
    p.add_argument("--binary", required=True)
    p.add_argument("--system-init", required=True)
    p.add_argument("--config", required=True)
    p.add_argument("--font", required=True)
    p.add_argument("--mono", required=True)
    p.add_argument("--mono-license", required=True)
    p.add_argument("--task-profiles", required=True)
    p.add_argument("--mtp-rc", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--extra", action="append", default=[], help="a folder whose tree is added to the ramdisk")
    a = p.parse_args()

    stock = open(a.stock, "rb").read()
    if stock[:8] != b"ANDROID!":
        raise SystemExit("the stock image is not an Android boot image")
    kernel_size, ramdisk_size = struct.unpack_from("<II", stock, 8)
    header_size = struct.unpack_from("<I", stock, 20)[0]
    header_version = struct.unpack_from("<I", stock, 40)[0]
    if header_version not in (3, 4):
        raise SystemExit(f"header version {header_version} predates the ramdisk-only layout")
    kernel = stock[PAGE : PAGE + kernel_size]
    ramdisk_at = PAGE + align(kernel_size)
    ramdisk = stock[ramdisk_at : ramdisk_at + ramdisk_size]
    entries = decode_ramdisk(ramdisk)
    print(f"stock: header v{header_version} ({header_size} bytes), kernel {len(kernel)}, ramdisk {len(ramdisk)}, {len(entries)} entries")

    replace(entries, "system/bin/recovery", open(a.binary, "rb").read())
    for command in COMMANDS:
        link(entries, f"system/bin/{command}", "recovery")
    insert(entries, "skyward/system-init", S_IFREG | 0o755, open(a.system_init, "rb").read())
    insert(entries, "skyward/device.toml", S_IFREG | 0o644, open(a.config, "rb").read())
    insert(entries, "skyward/MiSansVF.ttf", S_IFREG | 0o644, open(a.font, "rb").read())
    insert(entries, "skyward/KirkMono-Regular.ttf", S_IFREG | 0o644, open(a.mono, "rb").read())
    insert(entries, "skyward/KirkMono-OFL.txt", S_IFREG | 0o644, open(a.mono_license, "rb").read())
    if find(entries, "system/etc/task_profiles.json") is None:
        insert(entries, "system/etc/task_profiles.json", S_IFREG | 0o644, open(a.task_profiles, "rb").read())
    insert(entries, "system/etc/init/skyward-mtp.rc", S_IFREG | 0o644, open(a.mtp_rc, "rb").read())

    # A file with a suffix is data; one without is a program. The working
    # copy's own mode is no help here: on Windows nothing carries one, and
    # a program packed 644 does not start.
    data = {".so", ".xml", ".json", ".txt", ".conf", ".bin", ".ini"}
    for root in a.extra:
        base = Path(root)
        for path in sorted(base.rglob("*")):
            if not path.is_file():
                continue
            rel = path.relative_to(base).as_posix()
            mode = 0o644 if path.suffix in data else 0o755
            insert(entries, rel, S_IFREG | mode, path.read_bytes())

    props = find(entries, "prop.default")
    if props is None:
        raise SystemExit("prop.default is not in the ramdisk")
    props.data = without(props.data, BUILD_VERSION) + b"\nro.secure=0\nro.adb.secure=0\nro.debuggable=1\nro.ota.allow_downgrade=true\n"

    ramdisk = encode_ramdisk(entries)
    header = bytearray(stock[:PAGE])
    struct.pack_into("<I", header, 12, len(ramdisk))
    if header_version == 4:
        struct.pack_into("<I", header, 1580, 0)
    image = bytes(header) + kernel + b"\0" * (align(len(kernel)) - len(kernel)) + ramdisk
    image += b"\0" * (align(len(image)) - len(image))
    out = footed(image, "recovery", len(stock))
    open(a.out, "wb").write(out)
    print(f"wrote {a.out} ({len(out)} bytes, ramdisk {len(ramdisk)}, sha256 {hashlib.sha256(out).hexdigest()[:16]}...)")


if __name__ == "__main__":
    main()
