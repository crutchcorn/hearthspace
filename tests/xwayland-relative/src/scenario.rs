use crate::{
    client::{Clients, Events, Seen},
    evidence,
    server::Server,
    state::{Input, Snapshot},
    xwm::Wm,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};
use x11rb::protocol::xproto::{ConnectionExt, KeyButMask};

#[derive(Serialize)]
pub struct Failure {
    pub assertion: String,
    pub detail: String,
}

#[derive(Serialize)]
pub struct Report {
    selector: &'static str,
    pub failures: Vec<Failure>,
    identities: Value,
    frames: Vec<Value>,
    deliveries: Value,
    controls: Vec<Value>,
    protocol: Snapshot,
    cleanup_verified: bool,
}

impl Report {
    fn check(&mut self, ok: bool, assertion: &str, detail: impl std::fmt::Debug) {
        if !ok {
            self.failures.push(Failure {
                assertion: assertion.into(),
                detail: format!("{detail:?}"),
            });
        }
    }

    pub fn protocol_log(&mut self, path: &Path) -> Result<()> {
        let log = std::fs::read_to_string(path)?;
        let lines: Vec<_> = log.lines().collect();
        let relative: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                line.contains("zwp_relative_pointer_v1")
                    && line.contains(".relative_motion(")
                    && !line.contains("->")
            })
            .collect();
        let locked = lines.iter().any(|line| {
            line.contains("zwp_locked_pointer_v1")
                && line.contains(".locked()")
                && !line.contains("->")
        });
        self.check(
            locked,
            "protocol.lock_accepted_callback",
            "missing actual locked callback in Xwayland trace",
        );
        self.check(
            relative.len() == 3,
            "protocol.relative_callbacks",
            relative.len(),
        );
        if let (Some(first), Some(last)) = (relative.first(), relative.last()) {
            self.check(
                !lines[first.0..=last.0]
                    .iter()
                    .any(|line| line.contains("wl_pointer") && line.contains(".motion(")),
                "protocol.no_absolute_motion_during_relative_sequence",
                "unexpected wl_pointer.motion",
            );
        }
        self.controls.push(json!({"incoming_relative_callbacks": relative.iter().map(|(_, line)| *line).collect::<Vec<_>>(),
            "incoming_locked_callback": locked}));
        Ok(())
    }
}

fn wait_snapshot(server: &Server, ready: impl Fn(&Snapshot) -> bool) -> Result<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = server.send(Input::Inspect)?;
        if ready(&snapshot) {
            return Ok(snapshot);
        }
        ensure!(
            Instant::now() < deadline,
            "missing protocol readiness: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn surfaces(server: &Server, wm: &mut Wm, c: &Clients) -> Result<(u32, u32)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        wm.pump()?;
        let snapshot = server.send(Input::Inspect)?;
        if let (Some(a), Some(b)) = (wm.surfaces.get(&c.primary), wm.surfaces.get(&c.neighbor))
            && snapshot.surfaces.contains(a)
            && snapshot.surfaces.contains(b)
        {
            ensure!(a != b, "separate X clients must have distinct wl_surfaces");
            return Ok((*a, *b));
        }
        ensure!(
            Instant::now() < deadline,
            "X11/WL_SURFACE_ID pairing incomplete"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn button_checks(
    report: &mut Report,
    events: &Events,
    root: u32,
    window: u32,
    point: [i16; 2],
    label: &str,
) {
    report.check(
        evidence::button_integrity(events, root, window),
        &format!("integrity.{label}"),
        &events.buttons,
    );
    report.check(
        evidence::button_coordinates(events, point),
        label,
        &events.buttons,
    );
}

pub fn run(server: &Server, mut wm: Wm, artifacts: &Path) -> Result<Report> {
    let mut c = Clients::new()?;
    let (primary_surface, neighbor_surface) = surfaces(server, &mut wm, &c)?;
    wm.raise(c.primary)?;
    c.keyboard_marker()?;
    server.send(Input::Focus(primary_surface, 130., 120.))?;
    server.send(Input::Fence)?;
    Seen::fenced(&c)?;
    // Require an actual non-null Wayland cursor, not the default false value of
    // cursor_hidden. ShowCursor must have a visible owned image to restore.
    let visible = wait_snapshot(server, |s| {
        !s.cursor_hidden && s.cursor_surface.is_some() && s.locked_on.is_none()
    })?;
    c.hide_and_warp()?;
    let locked = wait_snapshot(server, |s| {
        s.locked_on == Some(primary_surface) && s.cursor_hidden
    })?;
    ensure!(
        locked.accepted_locks == 1,
        "unexpected lock recreation: {locked:?}"
    );
    c.grab()?;
    let start = c.pointer()?;
    ensure!(
        start.same_screen
            && start.root == c.root
            && start.child == c.child
            && [start.root_x, start.root_y] == [140, 120],
        "bad initial confined position: {start:?}"
    );
    // Flush initial warp/grab notifications through the same ingress fence.
    // No warps, grab changes, or ComputeFreezes-triggering requests between deltas.
    server.send(Input::Fence)?;
    Seen::fenced(&c)?;
    let mut report = Report {
        selector: crate::SELECTOR,
        failures: Vec::new(),
        identities: json!({"root": c.root, "primary": c.primary, "neighbor": c.neighbor,
            "child": c.child, "master": c.master, "relative": c.relative,
            "primary_surface": primary_surface, "neighbor_surface": neighbor_surface}),
        frames: Vec::new(),
        deliveries: json!({}),
        controls: vec![json!({"start": [140, 120], "child": [100, 100, 50, 50],
            "master_device": c.master, "relative_device": c.relative,
            "visible_cursor_baseline": visible})],
        protocol: locked,
        cleanup_verified: false,
    };

    for (index, (dx, x)) in [(100., 149), (-1., 148), (-1., 147)]
        .into_iter()
        .enumerate()
    {
        let snapshot = server.send(Input::Relative(dx))?;
        let seen = Seen::fenced(&c)?;
        let pointer = c.pointer()?;
        let point = [x, 120];
        report.check(
            pointer.same_screen && pointer.root == c.root && pointer.child == c.child,
            &format!("integrity.pointer.{index}"),
            pointer,
        );
        report.check(
            [pointer.root_x, pointer.root_y] == point && [pointer.win_x, pointer.win_y] == point,
            &format!("endpoint.{index}"),
            pointer,
        );
        report.check(
            evidence::motion_integrity(&seen.a, &c),
            &format!("integrity.motion.{index}"),
            &seen.a.motions,
        );
        report.check(
            seen.a
                .motions
                .iter()
                .all(|e| [e.root_x, e.root_y] == point && [e.event_x, e.event_y] == point),
            &format!("motion.{index}"),
            &seen.a.motions,
        );
        report.check(
            seen.b.motions.is_empty() && seen.b.buttons.is_empty() && seen.a.buttons.is_empty(),
            &format!("isolation.{index}"),
            &seen,
        );
        for (connection, events) in [("a", &seen.a), ("b", &seen.b)] {
            report.check(
                evidence::raw_integrity(events, &c, dx),
                &format!("raw.{connection}.{index}"),
                &events.raw,
            );
        }
        report.check(
            snapshot.locked_on == Some(primary_surface) && snapshot.accepted_locks == 1,
            &format!("lock.{index}"),
            &snapshot,
        );
        report.frames.push(json!({"delta": dx, "expected": point,
            "query_pointer": evidence::pointer(&pointer),
            "events": evidence::seen(&seen), "snapshot": snapshot}));
        // Preserve already observed endpoints if a later protocol/cleanup check
        // aborts. This partial report can never qualify as a passing matrix case.
        std::fs::write(
            artifacts.join("report.partial.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
    }

    server.send(Input::Click)?;
    let click = Seen::fenced(&c)?;
    report.deliveries["confined"] = evidence::seen(&click);
    button_checks(
        &mut report,
        &click.a,
        c.root,
        c.primary,
        [147, 120],
        "buttons.confined",
    );
    report.check(
        click.b.buttons.is_empty(),
        "buttons.confined_isolation",
        &click,
    );
    report.check(
        !c.pointer()?.mask.contains(KeyButMask::BUTTON1),
        "buttons.released",
        "Button1 stuck",
    );
    // Preserve the complete click evidence before release/unlock can fail.
    // The matrix still reads only report.json, never this partial checkpoint.
    std::fs::write(
        artifacts.join("report.partial.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    c.release()?;
    wait_snapshot(server, |s| {
        s.locked_on.is_none() && !s.cursor_hidden && s.cursor_surface == visible.cursor_surface
    })?;
    Seen::default().sync(&c)?;

    // Default-off control, independent of the live-confinement assertions:
    // visible focus remains A while a foreign root peer is stacked above it.
    wm.raise(c.neighbor)?;
    let stack = c.a.query_tree(c.root)?.reply()?.children;
    ensure!(
        stack.iter().position(|id| *id == c.neighbor)
            > stack.iter().position(|id| *id == c.primary),
        "recipient control requires neighbor above primary: {stack:?}"
    );
    server.send(Input::Focus(primary_surface, 60., 20.))?;
    server.send(Input::Click)?;
    let isolated = Seen::fenced(&c)?;
    report.deliveries["restacked"] = evidence::seen(&isolated);
    // A complete pair must reach exactly one real client even in default-off.
    // Recipient expectation is separate so a dropped/partial click can never
    // qualify as the expected default-off routing failure.
    report.check(
        (evidence::button_integrity(&isolated.a, c.root, c.primary)
            && isolated.b.buttons.is_empty())
            || (isolated.a.buttons.is_empty()
                && evidence::button_integrity(&isolated.b, c.root, c.neighbor)),
        "integrity.buttons.restacked",
        &isolated,
    );
    report.check(
        evidence::button_coordinates(&isolated.a, [60, 20])
            && evidence::button_coordinates(&isolated.b, [60, 20]),
        "coordinates.buttons.restacked",
        &isolated,
    );
    report.check(
        isolated.a.buttons.len() == 2,
        "recipient.primary_after_restack",
        &isolated,
    );
    report.check(
        isolated.b.buttons.is_empty(),
        "recipient.no_neighbor_leak",
        &isolated,
    );

    server.send(Input::Focus(neighbor_surface, 60., 20.))?;
    server.send(Input::Click)?;
    let neighbor = Seen::fenced(&c)?;
    report.deliveries["fresh_neighbor"] = evidence::seen(&neighbor);
    button_checks(
        &mut report,
        &neighbor.b,
        c.root,
        c.neighbor,
        [60, 20],
        "recipient.fresh_neighbor",
    );
    report.check(
        neighbor.a.buttons.is_empty(),
        "recipient.no_primary_leak",
        &neighbor,
    );
    c.cleanup_probe()?;
    let released = c.pointer()?;
    ensure!(
        !released.mask.contains(KeyButMask::BUTTON1),
        "held button after cleanup"
    );
    report.protocol = server.send(Input::Inspect)?;
    ensure!(report.protocol.locked_on.is_none(), "lock survived cleanup");
    report.check(
        report.protocol.relative_frames == vec![100., -1., -1.] && report.protocol.clicks == 3,
        "protocol.sequence",
        report.protocol.clone(),
    );
    report.cleanup_verified = true;
    Ok(report)
}
