//! The phone's own programs that use the keys its secure world keeps.
//! Which they are and how each is started is read from the installed
//! system's own partitions, as its init and its service directories read
//! them, so no phone is told of them by hand, and a system that names them
//! otherwise after an update is followed as it is.

use anyhow::{Context, Result};
use skyward_config::Service;
use skyward_programs::naming::{self, Does};
use skyward_programs::phone::{self, Partition, Phone, PARTITIONS};
use skyward_programs::secure::{self, Step};
use std::path::Path;

/// What the partitions of an installed system say that the recovery acts
/// on.
pub struct Found {
    /// Every service of the newer kind the phone describes: what its own
    /// service directory takes from the programs that register them, and
    /// what the recovery's is told of, so that each of the phone's
    /// programs registers there all it registers on the phone.
    pub described: Vec<String>,
    /// What to do to use the phone's keys, in order, or why the partitions
    /// do not say.
    pub steps: Result<Vec<Step>>,
}

/// What the partitions of the slot's installed system say. Which build of
/// the hardware a partition describes the phone's init may name itself as
/// the phone starts; where it does and the build has no name yet, the
/// recovery names it first, as the phone's scripts have init name it, and
/// reads the partitions as the phone so named.
///
/// `libraries` is where the libraries the phone's partitions were built
/// against are opened, on a phone that carries them as a bundle of their
/// own: each of its programs is given the folder among its own.
pub fn found(slot: skyward_vab::Slot, libraries: Option<&Path>) -> Result<Found> {
    let mut phone = read(slot, libraries)?;
    let unnamed = naming::steps(&phone)?;
    if !unnamed.is_empty() {
        name(&unnamed)?;
        phone = read(slot, libraries)?;
    }
    Ok(Found { described: phone.told_of(), steps: secure::steps(&phone) })
}

/// The phone as the partitions of the slot's installed system describe it,
/// read with the properties its init has as it reads them: the
/// bootloader's, with those the recovery has set beside them as the
/// phone's init sets them, and the installed build's. `libraries` is as
/// [`found`] takes it.
fn read(slot: skyward_vab::Slot, libraries: Option<&Path>) -> Result<Phone> {
    let properties = skyward_info::installed_props(slot)?;
    let mounted = skyward_vab::mounted_partitions(slot)?;
    let mut partitions = Vec::new();
    for name in PARTITIONS {
        let named = format!("{name}{}", slot.suffix());
        let partition = mounted.iter().find(|m| m.partition == named).with_context(|| format!("{named} is not mounted"))?;
        partitions.push(Partition::mounted(name, &partition.mount_point));
    }
    Phone::read(partitions, properties, libraries)
}

/// Do what the phone's init does to name the builds its partitions
/// describe: run each program to its end, and set each property to its
/// value as the properties stand by then, the programs having set theirs.
/// Init goes on however a program ends, and a property whose value names
/// one nobody has set stays unset, as init leaves it.
fn name(does: &[Does]) -> Result<()> {
    for one in does {
        match one {
            Does::Run(program) => {
                let status = crate::services::once(program)?;
                skyward_sys::kmsg(&format!("ran {} {}, which ended: {status}", program.service.path, program.service.args.join(" ")))?;
            }
            Does::Set { name, value } => match phone::expand(value, skyward_sys::prop) {
                Some(value) => {
                    skyward_sys::set_prop(name, &value)?;
                    skyward_sys::kmsg(&format!("{name} is {value}, as the phone's init sets it"))?;
                }
                None => skyward_sys::kmsg(&format!("{name} stays unset, as the phone's init leaves it: nothing has set what {value} names"))?,
            },
        }
    }
    Ok(())
}

/// The programs among `steps`, as the recovery keeps them running.
pub fn programs(steps: &[Step]) -> Vec<Service> {
    secure::programs(steps).into_iter().map(|started| started.service.clone()).collect()
}

/// The steps with what the configuration says of them: a program the
/// configuration names that the phone's partitions also name is the same
/// program, and takes what the configuration gives it that the partitions
/// cannot say — a library to load before the phone's own build of one asks
/// for an entry point the recovery's copy has not. A program of the
/// configuration's own that the partitions do not name is added after
/// them, and one taken over is not started a second time.
///
/// Nothing is taken from the partitions: what they say of the order, of
/// what each program registers, of the user and the groups it runs in, is
/// what a recovery follows, as it would on the phone itself.
pub fn with_config(steps: &[Step], extra: &mut Vec<Service>) -> Vec<Step> {
    let mut out: Vec<Step> = steps.to_vec();
    let mut alone: Vec<Service> = Vec::new();
    for service in extra.drain(..) {
        match out.iter_mut().find_map(|step| match step {
            Step::Start(started) if same(&started.service, &service) => Some(started),
            _ => None,
        }) {
            Some(started) => merge(&mut started.service, service),
            None => alone.push(service),
        }
    }
    for service in alone {
        out.push(Step::Start(Box::new(service.into())));
    }
    out
}

/// Whether the configuration's program is the one among `steps`: which of
/// the services it is ready with says so, where both name one, and its
/// path otherwise — any of them, as a phone may keep it under more than
/// one name.
fn same(step: &Service, named: &Service) -> bool {
    if let (Some(step), Some(named)) = (&step.ready_service, &named.ready_service) {
        return step == named;
    }
    named.paths().any(|path| path == step.path)
}

/// Give `into` what `from` says that it does not: what the partitions
/// cannot know of this phone's build of the program.
fn merge(into: &mut Service, from: Service) {
    if !from.alternates.is_empty() {
        into.alternates = from.alternates;
    }
    if !from.args.is_empty() {
        into.args = from.args;
    }
    if !from.libraries.is_empty() {
        into.libraries = from.libraries;
    }
    if !from.preload.is_empty() {
        let mut preload = from.preload;
        preload.append(&mut into.preload);
        into.preload = preload;
    }
    into.oneshot |= from.oneshot;
    if from.ready_property.is_some() {
        into.ready_property = from.ready_property;
    }
    if from.ready_service.is_some() {
        into.ready_service = from.ready_service;
    }
    if from.ready_hardware_service.is_some() {
        into.ready_hardware_service = from.ready_hardware_service;
    }
    for name in from.registers {
        if !into.registers.contains(&name) {
            into.registers.push(name);
        }
    }
    if from.when.is_some() {
        into.when = from.when;
    }
    if from.user.is_some() {
        into.user = from.user;
    }
    if !from.groups.is_empty() {
        into.groups = from.groups;
    }
    if !from.capabilities.is_empty() {
        into.capabilities = from.capabilities;
    }
}

/// Whether the recovery serves the secure element service among `steps`.
pub fn serves_elements(steps: &[Step]) -> bool {
    steps.contains(&Step::ServeElements)
}

/// Do `steps` in order: start each program and wait until it is ready,
/// and serve the secure element service where it stands among them, which
/// is after the secure elements' own services have registered and before
/// the weaver that reaches its secure element through it starts. Each
/// program names every folder its libraries are looked for in.
pub fn start(steps: &[Step]) -> Result<()> {
    for step in steps {
        match step {
            Step::Start(program) => {
                let element = program.service.ready_service.as_deref().is_some_and(|name| name.contains("ISecureElement"));
                match crate::services::start(std::slice::from_ref(program.as_ref()), None) {
                    Ok(()) => {}
                    // An element's service that does not come up is said so
                    // and left out: the programs after it, the weaver among
                    // them, are started either way, and say for themselves
                    // what they can do without it.
                    Err(e) if element => skyward_sys::kmsg(&format!("{} did not come up: {e:#}", program.service.path))?,
                    Err(e) => return Err(e),
                }
            }
            Step::ServeElements => {
                let readers = skyward_omapi::start()?;
                skyward_sys::kmsg(&format!("{} is served, with a reader for each of {} secure elements: {}", skyward_omapi::SERVICE, readers.len(), readers.join(", ")))?;
            }
        }
    }
    Ok(())
}

/// What `steps` are, a line each, as the `keys` command prints them.
pub fn describe(steps: &[Step]) -> Vec<String> {
    let mut lines = Vec::new();
    for step in steps {
        let Step::Start(program) = step else {
            lines.push(format!("serve {}", skyward_omapi::SERVICE));
            continue;
        };
        let s = &program.service;
        let mut line = format!("start {}", s.path);
        for word in &s.args {
            line.push_str(&format!(" {word}"));
        }
        line.push_str(&format!("\n  as {} in {}", s.user.as_deref().unwrap_or("root"), if s.groups.is_empty() { "root".to_string() } else { s.groups.join(" ") }));
        if !s.capabilities.is_empty() {
            line.push_str(&format!(", keeping {}", s.capabilities.join(" ")));
        }
        for socket in &program.sockets {
            let owner = format!("{}:{}", socket.user.as_deref().unwrap_or("root"), socket.group.as_deref().unwrap_or("root"));
            line.push_str(&format!("\n  with the socket {} ({:?}, {:o}, {owner})", socket.name, socket.kind, socket.mode));
        }
        for (name, value) in &program.environment {
            line.push_str(&format!("\n  with {name}={value}"));
        }
        for ready in s.ready_property.iter().chain(&s.ready_service).chain(&s.ready_hardware_service) {
            line.push_str(&format!("\n  ready with {ready}"));
        }
        for name in &s.registers {
            line.push_str(&format!("\n  registering {name} as well"));
        }
        if program.anew {
            line.push_str("\n  started anew where an earlier recovery service left it running");
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(path: &str) -> Service {
        Service {
            path: path.into(),
            alternates: Vec::new(),
            optional: false,
            args: Vec::new(),
            libraries: Vec::new(),
            preload: Vec::new(),
            oneshot: false,
            ready_property: None,
            ready_service: None,
            registers: Vec::new(),
            ready_hardware_service: None,
            when: None,
            user: None,
            groups: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    fn step(service: Service) -> Step {
        Step::Start(Box::new(service.into()))
    }

    fn paths(steps: &[Step]) -> Vec<String> {
        secure::programs(steps).iter().map(|p| p.service.path.clone()).collect()
    }

    /// A program the partitions name, as they name it.
    fn derived() -> Vec<Step> {
        let mut element = service("/vendor/bin/hw/android.hardware.secure_element-service.qti");
        element.ready_service = Some("android.hardware.secure_element.ISecureElement/eSE1".into());
        element.registers = vec!["android.hardware.secure_element.ISecureElement/SIM1".into()];
        element.user = Some("system".into());
        vec![step(service("/vendor/bin/qseecomd")), step(element)]
    }

    #[test]
    fn a_named_program_takes_the_place_of_the_one_the_partitions_name() {
        let mut named = service("/odm/bin/hw/android.hardware.secure_element-service.qti");
        named.alternates = vec!["/vendor/bin/hw/android.hardware.secure_element-service.qti".into()];
        named.ready_service = Some("android.hardware.secure_element.ISecureElement/eSE1".into());
        named.preload = vec!["/skyward/lib/libskyward_hal_shim.so".into()];
        let mut extra = vec![named];

        let steps = with_config(&derived(), &mut extra);
        assert_eq!(paths(&steps), ["/vendor/bin/qseecomd", "/vendor/bin/hw/android.hardware.secure_element-service.qti"], "the place the partitions give it");
        let started = secure::programs(&steps)[1];
        assert_eq!(started.service.preload, ["/skyward/lib/libskyward_hal_shim.so"], "what only the configuration knows");
        assert_eq!(started.service.registers, ["android.hardware.secure_element.ISecureElement/SIM1"], "what the phone says it registers");
        assert_eq!(started.service.user.as_deref(), Some("system"), "the user its own init runs it as");
        assert!(extra.is_empty(), "a program taken over is not started a second time");
    }

    #[test]
    fn a_program_the_partitions_do_not_name_comes_after_them() {
        let mut extra = vec![service("/vendor/bin/sh")];
        let steps = with_config(&derived(), &mut extra);
        assert_eq!(
            paths(&steps),
            ["/vendor/bin/qseecomd", "/vendor/bin/hw/android.hardware.secure_element-service.qti", "/vendor/bin/sh"],
            "the programs of the partitions first, in their order, and the configuration's own after them"
        );
        assert!(extra.is_empty());
    }

    #[test]
    fn what_the_configuration_does_not_name_is_left_as_the_partitions_have_it() {
        let mut extra = Vec::new();
        assert_eq!(with_config(&derived(), &mut extra), derived());
    }
}
