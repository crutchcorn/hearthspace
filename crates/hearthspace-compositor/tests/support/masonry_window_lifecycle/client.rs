use masonry::{
    app::RenderRootSignal,
    core::{ErasedAction, Widget, WidgetId},
    theme::default_property_set,
    widgets::{Button, ButtonPress},
};
use masonry_winit::app::{
    AppDriver, DriverCtx, EventLoop, EventLoopProxy, MasonryState, MasonryUserEvent, NewWindow,
    WindowId,
};
use xilem::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    platform::wayland::EventLoopBuilderExtWayland,
    window::{Window, WindowId as HandleId},
};

use super::{SURVIVOR, env, fs};

#[derive(Debug)]
struct Begin;
#[derive(Debug)]
struct Retire;

struct Driver {
    survivor: WindowId,
    victim: WindowId,
    survivor_handle: Option<HandleId>,
    proxy: EventLoopProxy,
    case: String,
    retired: bool,
    clicked: bool,
}

fn window(id: WindowId, title: &str) -> NewWindow {
    NewWindow::new_with_id(
        id,
        Window::default_attributes()
            .with_title(title)
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(240, 120)),
        Button::with_text("Still responsive").prepare().erased(),
    )
}

fn marker(name: &str) {
    fs::write(
        std::path::Path::new(&env::var_os("XDG_RUNTIME_DIR").unwrap()).join(name),
        "ok\n",
    )
    .unwrap();
}

impl Driver {
    fn retire(&mut self, ctx: &mut DriverCtx<'_>) {
        assert!(!self.retired);
        ctx.close_window(self.victim);
        self.retired = true;
        self.survivor_handle = Some(ctx.window(self.survivor).handle().id());
        ctx.render_root(self.survivor)
            .emit_signal(RenderRootSignal::RequestRedraw);
    }
}

impl AppDriver for Driver {
    fn on_start(&mut self, _: &mut MasonryState) {
        self.proxy
            .send_event(MasonryUserEvent::AsyncAction(
                self.survivor,
                Box::new(Begin),
            ))
            .unwrap();
    }

    fn on_async_action(&mut self, _: WindowId, ctx: &mut DriverCtx<'_>, action: ErasedAction) {
        assert!(action.is::<Begin>());
        if self.case == "first-frame" {
            // Same callback creates and retires a visible window before Masonry
            // drains its deferred first-presentation queue.
            ctx.create_window(window(self.victim, "Never presented"));
            self.retire(ctx);
            return;
        }
        let signal = if self.case == "animation-other" {
            RenderRootSignal::RequestAnimFrame
        } else {
            RenderRootSignal::RequestRedraw
        };
        ctx.render_root(self.victim).emit_signal(signal);
        let source = if self.case == "animation-other" {
            self.survivor
        } else {
            self.victim
        };
        // Actual RenderRoot channel and Masonry callback dispatch, with a
        // deterministic ordering equivalent to widget redraw followed by action.
        let root = ctx.render_root(source);
        let widget_id = root.get_layer_root(0).id();
        root.emit_signal(RenderRootSignal::Action(Box::new(Retire), widget_id));
    }

    fn on_action(
        &mut self,
        id: WindowId,
        ctx: &mut DriverCtx<'_>,
        _: WidgetId,
        action: ErasedAction,
    ) {
        if action.is::<Retire>() {
            self.retire(ctx);
        } else {
            assert!(action.is::<ButtonPress>());
            assert_eq!(id, self.survivor);
            assert!(self.retired);
            assert!(
                std::path::Path::new(&env::var_os("XDG_RUNTIME_DIR").unwrap())
                    .join("survivor-rendered")
                    .exists()
            );
            self.clicked = true;
            marker("survivor-clicked");
            ctx.exit();
        }
    }
}

struct Application {
    state: MasonryState,
    driver: Driver,
    resized: bool,
    rendered: bool,
}

impl ApplicationHandler<MasonryUserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.state.handle_resumed(event_loop, &mut self.driver);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MasonryUserEvent) {
        self.state
            .handle_user_event(event_loop, event, &mut self.driver);
        if self.driver.retired {
            // This returns only after the actual deferred work batch drains.
            // The parent now asks the compositor for a new size/configure.
            marker("retired-ready");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: HandleId, event: WindowEvent) {
        let survivor = self.driver.retired && self.driver.survivor_handle == Some(id);
        if survivor && let WindowEvent::Resized(size) = &event {
            self.resized |= size.width == 360 && size.height == 200;
        }
        let rendered = survivor && self.resized && matches!(event, WindowEvent::RedrawRequested);
        self.state
            .handle_window_event(event_loop, id, event, &mut self.driver);
        if rendered && !self.rendered {
            self.rendered = true;
            marker("survivor-rendered");
        }
    }
}

pub(super) fn run(case: String) {
    let event_loop = EventLoop::with_user_event()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    let survivor = WindowId::next();
    let victim = WindowId::next();
    let mut windows = vec![window(survivor, SURVIVOR)];
    if case != "first-frame" {
        windows.push(window(victim, "Retired window"));
    }
    let proxy = event_loop.create_proxy();
    let mut app = Application {
        state: MasonryState::new(proxy.clone(), windows, default_property_set()),
        driver: Driver {
            survivor,
            victim,
            survivor_handle: None,
            proxy,
            case,
            retired: false,
            clicked: false,
        },
        resized: false,
        rendered: false,
    };
    event_loop.run_app(&mut app).unwrap();
    assert!(app.resized && app.rendered && app.driver.clicked);
}
