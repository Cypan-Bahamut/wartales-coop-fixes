// Cypan's Wartales patch tool. Name-anchored bytecode patches on the
// byte-identical-roundtrip hlbc (wartales-mp's vendored 0.7.0 + fixes).
//
// Patch registry:
//   lootfix2 - co-op loot screen flicker: ui.win.Debrief.update rebuilds the
//     whole debrief window every frame in multiplayer whenever the repair/cure
//     affordability check disagrees with the built button state. The patch
//     makes the isMulti gate take the same early return as singleplayer.
//     One opcode field changes: op5 JTrue offset 1 -> 0.

use anyhow::{bail, Context, Result};
use hlbc::opcodes::Opcode;
use hlbc::types::{Function, RefType, Reg, Type};
use hlbc::Bytecode;
use std::io::Cursor;

/// Insert `new_ops` before original op index `at`, keeping every jump target,
/// debug line and variable-name assignment in `f` consistent.
/// (Same fixup algorithm as wartales-mp's patcher; reimplemented here.)
fn insert_ops(f: &mut Function, at: usize, new_ops: Vec<Opcode>) {
    let n = new_ops.len() as i64;
    let map = |i: i64| if i < at as i64 { i } else { i + n };
    for (i, op) in f.ops.iter_mut().enumerate() {
        let i = i as i64;
        let fix = |off: &mut i32| {
            let target = i + 1 + *off as i64;
            *off = (map(target) - map(i) - 1) as i32;
        };
        match op {
            Opcode::JTrue { offset, .. }
            | Opcode::JFalse { offset, .. }
            | Opcode::JNull { offset, .. }
            | Opcode::JNotNull { offset, .. }
            | Opcode::JSLt { offset, .. }
            | Opcode::JSGte { offset, .. }
            | Opcode::JSGt { offset, .. }
            | Opcode::JSLte { offset, .. }
            | Opcode::JULt { offset, .. }
            | Opcode::JUGte { offset, .. }
            | Opcode::JNotLt { offset, .. }
            | Opcode::JNotGte { offset, .. }
            | Opcode::JEq { offset, .. }
            | Opcode::JNotEq { offset, .. }
            | Opcode::JAlways { offset }
            | Opcode::Trap { offset, .. } => fix(offset),
            Opcode::Switch { offsets, end, .. } => {
                for o in offsets.iter_mut() {
                    fix(o);
                }
                fix(end);
            }
            _ => {}
        }
    }
    if let Some(dbg) = &mut f.debug_info {
        let line = dbg[at.saturating_sub(1)];
        for _ in 0..n {
            dbg.insert(at, line);
        }
    }
    if let Some(assigns) = &mut f.assigns {
        let len = f.ops.len();
        for (_, pos) in assigns.iter_mut() {
            if *pos < len && *pos >= at {
                *pos += n as usize;
            }
        }
    }
    for (k, op) in new_ops.into_iter().enumerate() {
        f.ops.insert(at + k, op);
    }
}

fn obj_type(code: &Bytecode, name: &str) -> Result<RefType> {
    code.types
        .iter()
        .position(|t| matches!(t, Type::Obj(o) if code.strings[o.name.0].as_str() == name))
        .map(RefType)
        .with_context(|| format!("type {name} not found"))
}

/// Index into code.functions of method `name` whose `this` is `this_t`.
fn method_index(code: &Bytecode, this_t: RefType, name: &str) -> Result<usize> {
    let mut hits = code.functions.iter().enumerate().filter(|(_, f)| {
        code.strings[f.name.0].as_str() == name
            && f.t.as_fun(code).and_then(|ft| ft.args.first().copied()) == Some(this_t)
    });
    let (i, _) = hits
        .next()
        .with_context(|| format!("method {name} on type {} not found", this_t.0))?;
    if hits.next().is_some() {
        bail!("method {name} on type {} is ambiguous", this_t.0);
    }
    Ok(i)
}

fn load(path: &str) -> Result<(Vec<u8>, Bytecode)> {
    let image = std::fs::read(path).context("read input")?;
    let code = Bytecode::deserialize(&mut Cursor::new(&image)).context("parse bytecode")?;
    Ok((image, code))
}

fn save(code: &Bytecode, image_len: usize, path: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(image_len + 64);
    code.serialize(&mut out).context("serialize")?;
    std::fs::write(path, &out).context("write output")?;
    Ok(out)
}

/// Locates the lootfix2 site with three independent anchors and returns
/// (function index). Fails loudly on any game build where the code moved.
fn find_lootfix2_site(code: &Bytecode) -> Result<usize> {
    let debrief_t = obj_type(code, "ui.win.Debrief")?;
    let fi = method_index(code, debrief_t, "update")?;
    let f = &code.functions[fi];
    // Anchor 2: op4 calls a function named get_isMulti.
    match &f.ops[4] {
        Opcode::Call1 { fun, .. } => {
            let callee = code
                .functions
                .iter()
                .find(|g| g.findex == *fun)
                .context("isMulti callee not found")?;
            if code.strings[callee.name.0].as_str() != "get_isMulti" {
                bail!(
                    "op4 calls {}, expected get_isMulti",
                    code.strings[callee.name.0]
                );
            }
        }
        other => bail!("op4 is not Call1: {other:?}"),
    }
    // Anchor 3: op5 is the gate jump, op6 the early return.
    match (&f.ops[5], &f.ops[6]) {
        (Opcode::JTrue { offset, .. }, Opcode::Ret { .. }) if *offset == 0 || *offset == 1 => Ok(fi),
        (a, b) => bail!("unexpected gate shape: op5={a:?} op6={b:?}"),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("roundtrip") if args.len() == 4 => {
            let (image, code) = load(&args[2])?;
            let out = save(&code, image.len(), &args[3])?;
            println!(
                "roundtrip {} -> {} bytes ({})",
                image.len(),
                out.len(),
                if image == out {
                    "BYTE-IDENTICAL"
                } else {
                    "DIFFERS"
                }
            );
            Ok(())
        }
        Some("lootfix2") if args.len() == 4 => {
            let (image, mut code) = load(&args[2])?;
            let fi = find_lootfix2_site(&code)?;
            match &mut code.functions[fi].ops[5] {
                Opcode::JTrue { offset, .. } => {
                    if *offset == 0 {
                        bail!("already patched");
                    }
                    *offset = 0;
                }
                _ => unreachable!(),
            }
            let out = save(&code, image.len(), &args[3])?;
            let diff: Vec<usize> = (0..image.len().min(out.len()))
                .filter(|&i| image[i] != out[i])
                .collect();
            println!(
                "lootfix2 applied to fn #{fi}: {} bytes, {} byte(s) changed at {:?}",
                out.len(),
                diff.len(),
                diff
            );
            if image.len() != out.len() || diff.len() != 1 {
                bail!("expected exactly one changed byte at equal length; NOT SAFE, discard output");
            }
            Ok(())
        }
        Some("verify") if args.len() == 3 => {
            let (_, code) = load(&args[2])?;
            let fi = find_lootfix2_site(&code)?;
            match &code.functions[fi].ops[5] {
                Opcode::JTrue { offset, .. } if *offset == 0 => println!("lootfix2: PRESENT"),
                Opcode::JTrue { offset, .. } if *offset == 1 => println!("lootfix2: absent (vanilla)"),
                other => println!("lootfix2: unknown state {other:?}"),
            }
            Ok(())
        }
        Some("nightmarefix") if args.len() == 4 => {
            // Co-op fog battles: guests never locally recompute unit visibility
            // (tryUpdateVisibility is auth-gated), so a missed/early reveal RPC
            // leaves a reinforcement permanently untargetable on the guest.
            //   1. tryUpdateVisibility: drop the isAuth early-return, and turn
            //      its net echo into a local visibility re-apply.
            //   2. Skill.gatherTargets: refresh each candidate unit's visibility
            //      (now allowed on any client) right before validity testing.
            let (image, mut code) = load(&args[2])?;
            let unit_t = obj_type(&code, "battle.Unit")?;
            let try_fi = method_index(&code, unit_t, "tryUpdateVisibility")?;
            let try_findex = code.functions[try_fi].findex;
            let net_fi = method_index(&code, unit_t, "netUpdateVisibility")?;
            let net_findex = code.functions[net_fi].findex;
            let impl_fi = method_index(&code, unit_t, "netUpdateVisibility__impl")?;
            let impl_findex = code.functions[impl_fi].findex;
            let valid_fi = method_index(&code, unit_t, "isValidTarget")?;
            let valid_findex = code.functions[valid_fi].findex;
            // null<bool> register type: second arg of tryUpdateVisibility.
            let null_bool_t = code.functions[try_fi]
                .t
                .as_fun(&code)
                .context("tryUpdateVisibility type")?
                .args[1];

            // --- 1. tryUpdateVisibility ---
            {
                let f = &mut code.functions[try_fi];
                match (&f.ops[5], &f.ops[6], &f.ops[7]) {
                    (
                        Opcode::Field { .. },
                        Opcode::JTrue { offset: 1, .. },
                        Opcode::Ret { .. },
                    ) => {}
                    (a, b, c) => bail!("tryUpdateVisibility gate shape changed: {a:?} {b:?} {c:?}"),
                }
                f.ops[6] = Opcode::JAlways { offset: 1 };
                match &mut f.ops[17] {
                    Opcode::Call2 { fun, .. } if *fun == net_findex => *fun = impl_findex,
                    other => bail!("expected net echo Call2 at op17, got {other:?}"),
                }
            }

            // --- 2. gatherTargets: insert refresh before each isValidTarget ---
            let skill_t = obj_type(&code, "battle.skill.Skill")?;
            let gather_fi = method_index(&code, skill_t, "gatherTargets")?;
            let f = &mut code.functions[gather_fi];
            let void_reg = Reg(f
                .regs
                .iter()
                .position(|t| t.0 == 0)
                .context("no void register in gatherTargets")? as u32);
            f.regs.push(null_bool_t);
            let null_reg = Reg((f.regs.len() - 1) as u32);
            let sites: Vec<(usize, Reg)> = f
                .ops
                .iter()
                .enumerate()
                .filter_map(|(i, op)| match op {
                    Opcode::Call3 { fun, arg0, .. } if *fun == valid_findex => Some((i, *arg0)),
                    _ => None,
                })
                .collect();
            if sites.is_empty() {
                bail!("no isValidTarget call sites found in gatherTargets");
            }
            let nsites = sites.len();
            for &(i, unit_reg) in sites.iter().rev() {
                insert_ops(
                    f,
                    i,
                    vec![
                        Opcode::Null { dst: null_reg },
                        Opcode::Call2 {
                            dst: void_reg,
                            fun: try_findex,
                            arg0: unit_reg,
                            arg1: null_reg,
                        },
                    ],
                );
            }

            let out = save(&code, image.len(), &args[3])?;
            // Re-parse the output as a final sanity gate.
            let reparsed = Bytecode::deserialize(&mut Cursor::new(&out))
                .context("re-parse of patched output failed")?;
            println!(
                "nightmarefix applied: gate ungated, echo localized, {} refresh sites in gatherTargets; output {} bytes, {} functions",
                nsites,
                out.len(),
                reparsed.functions.len()
            );
            Ok(())
        }
        Some("vtable") if args.len() == 5 => {
            let (_, code) = load(&args[2])?;
            let want: i32 = args[4].parse()?;
            let mut t = Some(obj_type(&code, &args[3])?);
            while let Some(ct) = t {
                let o = match &code.types[ct.0] {
                    Type::Obj(o) => o,
                    _ => break,
                };
                for p in &o.protos {
                    if p.pindex == want {
                        println!(
                            "pindex {} = {}.{} -> fn@{}",
                            want,
                            code.strings[o.name.0],
                            code.strings[p.name.0],
                            p.findex.0
                        );
                    }
                }
                t = o.super_;
            }
            Ok(())
        }
        _ => bail!("usage: patchtool roundtrip|lootfix2 <in.dat> <out.dat> | verify <file.dat> | vtable <file.dat> <Type> <pindex>"),
    }
}
