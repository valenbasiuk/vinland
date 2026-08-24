// handlers/xwm.rs
// manejo de ventanas y shells X11 via Xwayland

use smithay::input::dnd::DndGrabHandler;
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::{X11Surface, X11Wm, XwmHandler, xwm::{ResizeEdge as X11ResizeEdge, XwmId}};
use tracing::info;

use crate::state::Vinland;

impl DndGrabHandler for Vinland {}

impl XWaylandShellHandler for Vinland {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }
}

impl XwmHandler for Vinland {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().unwrap()
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {
        info!("[X11] nueva ventana X11 detectada");
    }

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {
        info!("[X11] nueva ventana override_redirect X11 detectada");
    }

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        info!("[X11] map_window_request recibido");
        window.set_mapped(true).unwrap();

        // para ventanas normales x11 (no override_redirect), centrarlas en pantalla
        // respetando los gaps del compositor y aplicando reglas de ventana si coinciden
        if !window.is_override_redirect() {
            let class = window.class();
            let instance = window.instance();
            let title = window.title();
            let app_id = if !class.is_empty() {
                Some(class.as_str())
            } else if !instance.is_empty() {
                Some(instance.as_str())
            } else {
                None
            };

            let matched_rule = self
                .config
                .rules
                .iter()
                .find(|r| r.matches(app_id, Some(title.as_str())))
                .cloned();

            let gap = self.config.tiling.gap;
            let out_size = self.backend.window_size();
            let mut geo = window.geometry();

            if let Some(ref rule) = matched_rule {
                if let Some(size) = rule.size {
                    geo.size.w = size[0];
                    geo.size.h = size[1];
                }
            }

            let x = ((out_size.w - geo.size.w) / 2).max(gap);
            let y = ((out_size.h - geo.size.h) / 2).max(gap);
            let _ = window.configure(Some(smithay::utils::Rectangle::new(
                (x, y).into(),
                geo.size,
            )));
        }
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {
        info!("[X11] mapped_override_redirect_window");
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        info!("[X11] ventana X11 desmapeada");
        if !window.is_override_redirect() {
            window.set_mapped(false).unwrap();
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, _window: X11Surface) {
        info!("[X11] ventana X11 destruida");
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<smithay::xwayland::xwm::Reorder>,
    ) {
        let mut geo = window.geometry();
        if let Some(x) = x { geo.loc.x = x; }
        if let Some(y) = y { geo.loc.y = y; }
        if let Some(w) = w { geo.size.w = w as i32; }
        if let Some(h) = h { geo.size.h = h as i32; }
        let _ = window.configure(Some(geo));
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
        _above: Option<u32>,
    ) {}

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _edges: X11ResizeEdge,
    ) {}

    fn move_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
    ) {}
}
