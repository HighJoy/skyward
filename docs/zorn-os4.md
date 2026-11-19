# REDMI K80 (zorn) 适配 HyperOS 4.0

本 fork 只做一件事：把 `devices/zorn.toml` 的构建基线从 HyperOS 3 换到 HyperOS 4.0，
并让打包工具能直接吃一个已经下载好的完整卡刷包。除下面列出的文件外，源码一行未动——
解密链路、安装流程、UI、双系统全部沿用上游 `kirklin/skyward`。

## 底包

| 项 | 值 |
| --- | --- |
| 机型 / 代号 | REDMI K80 / `zorn`（大陆版） |
| 系统版本 | `OS4.0.3.0.XOKCNXM`（HyperOS 4.0，Android 17） |
| 卡刷包 | `zorn-ota_full-OS4.0.3.0.XOKCNXM-user-17.0-3e3ae497fc.zip` |
| 包大小 | 7,799,326,368 字节 |
| 下载地址 | `https://bkt-sgp-miui-ota-update-alisgp.oss-ap-southeast-1.aliyuncs.com/OS4.0.3.0.XOKCNXM/zorn-ota_full-OS4.0.3.0.XOKCNXM-user-17.0-3e3ae497fc.zip` |
| 原始地址 | `https://bigota.d.miui.com/OS4.0.3.0.XOKCNXM/zorn-ota_full-...zip`（中国大陆以外只回 502，故用上面同一文件的新加坡镜像） |
| 包内 `payload.bin` | 7,799,317,771 字节，41 个分区，manifest 289,842 字节 |
| recovery 分区 | 104,857,600 字节（整分区，末尾 AVB footer） |
| recovery SHA-256 | `c0ca7a533fcacc09d693066179671e4968c8e2e4fc8a7a76f3b22cd2316902c9` |

recovery 分区的哈希取自 `payload.bin` 的 manifest（`new_partition_info.hash`），
与按 payload 操作把该分区完整还原后算出的 SHA-256 一致，也与 CI 里
`sha256sum --check` 的结果一致。

Skyward 的镜像就是**在这个原厂 recovery 上改的**：原厂的 init、adbd、SELinux 策略、
库全部保留，只替换 ramdisk 里的 recovery 服务，所以底包必须与手机实际运行的
系统版本对得上。

## 改动的文件

共 8 个文件，`git diff --stat c5e09f6..HEAD`：

| 文件 | 改动 |
| --- | --- |
| `devices/zorn.toml` | `[stock]` 三段改为 OS4.0.3.0：`version`、`package`（新加坡镜像）、`recovery`（新的 SHA-256）；补 `[firmware] partition = "modem"`、`[partitions] mounted = ["persist"]`（依据原厂 `fstab.qcom`）、`[backup]`、`[wipe]`（照 17 Ultra）、`display.diagonal = 6.67`、`display.corner_radius = 140`（照 13 Ultra）；`display.density` 由 560 改为 600（依据实机 `persist.sys.miui_resolution`）；去掉 `[wlan].firmware` 里的 `wlan_mac.bin`（实机 persist 里没有这个文件）；触摸、振动、post_boot 服务未动 |
| `README.md` | 支持列表里 K80 一行：`OS3.0.309.0.WOKCNXM (Android 16)` → `OS4.0.3.0.XOKCNXM (Android 17)` |
| `README.zh-CN.md` | 同上 |
| `crates/skyward-config/src/lib.rs` | `each_phone_names_the_stock_build_its_recovery_is_made_on` 里 zorn 的期望版本号跟着改；否则测试与配置互相对不上 |
| `crates/skyward-pack/src/main.rs` | 新增 `open_package()`：`package` 是本机存在的文件就用 `Payload::open` 读文件，否则按 URL 走 `Payload::open_url`。原来只有 URL 一条路 |
| `crates/skyward-pack/src/described.rs` | 改用 `crate::open_package()`，与上面一致 |
| `crates/skyward-bootimg/src/ramdisk.rs` | 把 `Ramdisk::unpack`、`c_path`、`set_time` 以及用到它们的两个测试加 `#[cfg(unix)]`；非 Unix 主机上补 `allow(dead_code)` / `allow(unused_imports)`。**Unix（含 Linux CI、Android）行为完全不变** |
| `.github/workflows/k80.yml` | 新增：fork 自用的 CI，只对 `zorn` 跑 lint/测试/打包；上游的 `build.yml`（21 个机型全量）在 fork 上手动禁用了 |

## 打进去的东西

`skyward-pack recovery` 在原厂 recovery 的 ramdisk（976 个 cpio 条目）上做这些：

| 动作 | 内容 |
| --- | --- |
| 替换 | `system/bin/recovery` → 编出来的 Skyward（aarch64，8,806,936 字节） |
| 新增链接 | `system/bin/7za`、`avbctl`、`resetprop`、`unzip`、`zstd` → 指向 `recovery` |
| 新增 | `skyward/system-init`（静态链接，1,045,656 字节） |
| 新增 | `skyward/device.toml`（即 `devices/zorn.toml`） |
| 新增 | `skyward/MiSansVF.ttf`（20,093,424）、`skyward/KirkMono-Regular.ttf`、`skyward/KirkMono-OFL.txt` |
| 新增 | `system/etc/task_profiles.json`（1,798 字节，原厂 ramdisk 没有它，logd 会起不来） |
| 新增 | `system/etc/init/skyward-mtp.rc`（1,245 字节） |
| 改写 | `prop.default`：去掉 `ro.build.version.release`、`ro.build.version.security_patch`、`ro.vendor.build.security_patch`（留给 recovery 从已装系统里补，否则密钥会被安全世界拒绝），并追加 `ro.secure=0`、`ro.adb.secure=0`、`ro.debuggable=1`、`ro.ota.allow_downgrade=true` |
| 重新打包 | cpio → LZ4 legacy 帧，更新 boot header 的 `ramdisk_size`，按分区大小补齐，末尾写 unsigned AVB footer（`add_hash_footer --algorithm NONE` 的布局） |

## 在 OS4.0.3.0 上核对过的

解密依赖的程序按配置和原厂固件逐条核对，OS4 全部沿用，没有改名或挪位置：

| 服务 | OS4.0.3.0 里的样子 |
| --- | --- |
| keymint | `aidl v3`，`android.hardware.security.keymint.IKeyMintDevice/default`，`/vendor/bin/hw/android.hardware.security.keymint-service-qti` |
| gatekeeper | `IGatekeeper/default`，`/vendor/bin/hw/android.hardware.gatekeeper-service-qti` |
| weaver | `/odm/bin/hw/android.hardware.weaver-service.nxp-qti` |
| qseecomd | `/vendor/bin/qseecomd` |
| 性能调度 | `kernel-post-boot` = `/vendor/bin/sh /vendor/bin/init.kernel.post_boot.sh`，`user root`、`groups root system wakelock graphics`，与配置一致 |
| WLAN | `/vendor/etc/wifi/kiwi_v2/WCNSS_qcom_cfg.ini`、`/vendor/bin/hw/wpa_supplicant` 都在；`qca_cld3_kiwi_v2.ko` 在，`qca_cld3_kiwi` 仍被 blocklist；`/vendor/firmware/wlan/qca_cld/kiwi_v2/wlan_mac.bin` 指向 `/mnt/vendor/persist/wlan/wlan_mac.bin`，配置里的 `wlan/wlan_mac.bin` 由此确认（不是 13 Ultra 的 `kiwi_v2/wlan_mac.bin`） |
| 触摸驱动 | `focaltech_touch.ko` 仍在 `vendor_dlkm` |
| 显示 | `vendor.display.default_resolution=1440,3200`，`persist.sys.miui_resolution=1080,2400,420`（密度 420 → 配置里 560 仍成立） |
| 挂载 | 原厂 `/vendor/etc/fstab.qcom`：`modem → /vendor/firmware_mnt`（`wait,slotselect`）、`persist → /mnt/vendor/persist`（`wait,check`）。recovery 不执行 init 的这些挂载，所以由 Skyward 自己挂：modem 给 WLAN 驱动取固件，persist 给 qseecomd 的监听器放安全世界的文件（含解密要用的不可回滚密钥） |

## 和两台真机机型对齐的部分

仓库里只有 17 Ultra（`nezha`，OS4.0.4.0）和 13 Ultra（`ishtar`）是作者在真机上验证过的，
它们的配置是唯一可信的参照。K80 这次照它们补齐了：

| 项 | 来源 | 说明 |
| --- | --- | --- |
| `[firmware]` / `[partitions]` | 17 Ultra | 原厂 fstab 把 modem、persist 都标 `wait`，recovery 不跑 init 的挂载 |
| `[backup]` | 17 Ultra | efs 五个分区两台真机一致；`data_skipped` 取两台并集，含 goodix 与 fpc 两家的指纹目录 |
| `[wipe]` | 17 Ultra | 9 个日志目录，含 Qualcomm 的 `bsplog`、`ramdump_ssr` |
| `display.corner_radius` | 13 Ultra | 两块屏的圆角一致，取 140 |
| `display.diagonal` | 机型规格 | 6.67 英寸 |

**不填的两项，理由已定：**

| 项 | 为什么 |
| --- | --- |
| `[logd]` | 不需要：K80 原厂 recovery 的 logd 依赖闭包共 16 个 `.so`，ramdisk 里全都有（含 `libsysutils`、`libcap`）；17 Ultra 缺的正是这两个，K80 不缺 |
| `[[owners]]` | 不需要：13 Ultra 要它，是因为它的面板只报原始帧、必须跑厂家的触摸程序；K80 让驱动自己报坐标，同 17 Ultra 的路子 |

**只剩一项要在真机上试：**

| 项 | 为什么不能照抄 |
| --- | --- |
| `haptics.click` / 强度 | 17 Ultra 用采样合成、13 Ultra 用驱动自带波形 0；K80 马达的实际谐振（`lra_frequency_hz = 1749`，即 174.9 Hz）与手感要现场试 |

## 实机信息核对（OS4.0.3.0，已 root 的运行系统）

在手机上跑 `out/collect-zorn-su.sh`（脚本不在仓库里）读出的事实，以及据此改动：

| 读到 | 值 | 据此 |
| --- | --- | --- |
| `persist.sys.miui_resolution` | `1440,3200,600` | `display.density` 由 560 改为 **600** |
| `ro.boot.hwc` / `ro.boot.product.vendor.sku` | `CN` / `pineapple` | 读的正是 `manifest_pineapple.xml`（gatekeeper 在那里） |
| `by-name` 里的 `modemst1/2`、`fsg`、`fsc`、`persist` | 五个都在 | `[backup].efs` 成立 |
| `persist.vendor.sys.fp.vendor` | `goodix_us` | 指纹是 goodix；`data_skipped` 里 goodix 必须有，fpc 一并留着无害 |
| `/data/vendor` 与 `/data/vendor/misc` | `tzstorage`、`qtee`、`mitee`、`goodix`、`fpc`、`fingerprint`、`fpdata`、`misc/mi_fp` 都在 | `data_skipped` 八项全部命中 |
| `/sys/class/touch/touch_dev/enable_touch_raw` | 存在，`system:system` 0664 | `[touch].writes` 的节点名正确 |
| 息屏 | 面板进低功耗（LP2）后 FocalTech 触摸控制器跟着掉电，不再随面板醒来 | `[display].rests = false`：面板保持点亮、只关背光，黑帧盖住画面，点一下即亮 |
| `system_dlkm` 的模块 | 里面有两套：`flatten` 是 AOSP 内核的，`/system_dlkm/lib/modules/<uname -r>` 才是本机的 | `source_dir` 改为按 `/proc/sys/kernel/osrelease` 取；`flatten` 的 `rfkill.ko` 导出保护符号，内核拒收，连带 `cfg80211`、`qca_cld3_kiwi_v2` 全部加载失败 |
| 密钥服务起不来 | weaver/keymint/gatekeeper 按系统的新 libc++ 编译，recovery 自带的旧版缺 `__libcpp_verbose_abort` | 启动 `vendor`、`odm` 的 `bin/hw` 程序时把 `/mnt/system/system/lib64/libc++.so` 放进 `LD_PRELOAD`；把系统库目录塞进 `LD_LIBRARY_PATH` 反而会缺十几个人家没有的库 |
| `lra_frequency_hz` | `1749`（十分之一赫兹，即 174.9 Hz） | `[haptics].resonance` 路径正确 |
| `/vendor/lib/modules` | `focaltech_touch.ko`、`qca_cld3_kiwi_v2.ko` | `[modules].load`、`[wlan].modules` 正确 |
| `/mnt/vendor/persist/wlan` | **空目录**；`persist/kiwi_v2` 不存在 | `[wlan].firmware` 去掉 `wlan_mac.bin` 一项：文件不在，`place()` 读它会报错并中断 WLAN 启动；地址由芯片自己给 |

## 解密

`/data` 的密钥在 weaver 的槽位里，weaver 经 OMAPI 读它，而 recovery 里没有 framework 去提供 OMAPI。这一版带上了三件从手机自带 TWRP 里取来的东西，都放在 `ports/zorn/ramdisk/`，打包时用 `--extra` 放进 ramdisk：

| 文件 | 作用 |
| --- | --- |
| `skyward/omapi/lib64/libskyward_omapi_compat.so` | 兼容层。没有它，高通的 `secure_element-service.qti` 每秒退出一次，一个元件也不注册 |
| `skyward/omapi/se_omapi` | recovery 自己的 OMAPI 服务，为每个已注册的安全元件建一个 reader |
| `skyward/nxp/weaver` + `lib64/ese_weaver.so` | NXP 的 weaver v2，经 OMAPI 读槽位；`/skyward/nxp/lib64` 里还有它要的 AIDL v2 桩与 keymint transport |

链路是：`secure_element-service.qti`（先加载兼容层）注册 `ISecureElement/eSE1` → `se_omapi` 为它建 reader → `weaver` 读槽位。实测日志 `weaver: Read slot 0 OK`，锁屏密码能解开 `/data`。

`[service]` 里点名了这三个程序之后，Skyward 不再从分区自己推导密钥步骤（`the keys are used by the programs the configuration names`），否则它会再起一个手机自带的 weaver，两个抢服务目录里同一个位置。

## 验证

- CI（`.github/workflows/k80.yml`）三步全绿：`cargo clippy --workspace --all-targets -D warnings`、`cargo test --workspace`、手机目标 clippy，以及取原厂 recovery → 校验 SHA-256 → 打包 → 压 zip。
  产物：`skyward-<sha7>-zorn-OS4.0.3.0.XOKCNXM.zip`（约 44.7 MB），在 run 页面的 Artifacts 里。
- AVB 页脚：用上游 `crates/skyward-pack/tests/avb-footer-*.bin`（avbtool 1.4.0 生成）逐字节比对通过。
- 真机已验证：进 recovery、触摸（含熄屏再点亮）、WLAN、`/data` 解密。振动与 fastbootd 的触摸仍未验证。

## 复现

```sh
cargo ndk -t arm64-v8a --platform 30 build --release -p skyward
RUSTFLAGS="-C target-feature=+crt-static" cargo ndk -t arm64-v8a --platform 30 build --release \
  -p skyward-system-init --target-dir target/system-init
cargo build --release -p skyward-pack

target/release/skyward-pack stock --config devices/zorn.toml --out target/stock/recovery.img
target/release/skyward-pack recovery \
  --stock target/stock/recovery.img \
  --binary target/aarch64-linux-android/release/skyward \
  --system-init target/system-init/aarch64-linux-android/release/system-init \
  --config devices/zorn.toml \
  --font resources/fonts/MiSansVF.ttf \
  --mono resources/fonts/KirkMono-Regular.ttf \
  --mono-license resources/fonts/KirkMono-OFL.txt \
  --extra ports/zorn/ramdisk \
  --out target/skyward-zorn.img
```

刷入（需已解锁 bootloader，刷当前槽位的 recovery）：

```sh
fastboot flash recovery skyward-<sha7>-zorn-OS4.0.3.0.XOKCNXM.img
fastboot reboot recovery
```
