// seat handler -> wl_seat, wl_keyboard, wl_pointer
// viaje del input en general desde el os hasta las apps

use smithay::backend::input::{
    self, Axis, AxisSource, Event, InputBackend, InputEvent,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, AbsolutePositionEvent,
};
use smithay::input::{
    SeatHandler, SeatState, Seat,
    pointer::{CursorImageStatus, MotionEvent, ButtonEvent, AxisFrame},
    keyboard::FilterResult,
};
use smithay::reexports::wayland_server::protocol::{wl_surface::WlSurface, wl_pointer};
use smithay::utils::{IsAlive, Logical, Point, SERIAL_COUNTER};


use crate::handlers::layer_shell::layer_surface_geometry;
use crate::state::Vinland;
use smithay::utils::Size;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::info;

use smithay::reexports::wayland_server::Resource;
use smithay::desktop::utils::under_from_surface_tree;
use smithay::desktop::PopupManager;
use smithay::desktop::WindowSurfaceType;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;

// seathandler -> define qué tipos reciben foco de cada dispositivo
impl SeatHandler for Vinland {
    type KeyboardFocus = WlSurface;
    type PointerFocus  = WlSurface;
    type TouchFocus    = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Vinland> {
        &mut self.seat_state
    }

    // llamado cuando el foco del teclado cambia entre superficies
    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        // obtenemos el cliente wayland que es dueño de la superficie
        let focus = focused.and_then(|s| dh.get_client(s.id()).ok());
        // sincronizamos el foco del portapapeles/data device con el cliente actual
        smithay::wayland::selection::data_device::set_data_device_focus(dh, seat, focus);
    }

    // llamado cuando una app pide cambiar la imagen del cursor
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.cursor_status = image;
    }
}

impl Vinland {
    // process_input_event -> traduce eventos del backend a protocolos wayland
    // winit genera InputEvent<WinitInput>, nosotros los mapeamos al seat
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        match event {


            // tecla presionada o soltada
            InputEvent::Keyboard { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time   = Event::time_msec(&event);
                let keyboard = self.seat.get_keyboard().unwrap();                // input() -> despacha el evento al cliente con foco actual.
                // el closure recibe (mods, handle) y decide si interceptar (Intercept) o pasar (Forward).
                keyboard.input(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |state, mods, handle| {
                        use smithay::backend::input::KeyState;
                        use crate::config::KeyAction;

                        state.super_pressed = mods.logo;

                        // solo actuamos en key press, no en release
                        if event.state() != KeyState::Pressed {
                            return FilterResult::Forward;
                        }

                        let raw_syms = handle.raw_syms();
                        let mod_sym = handle.modified_sym();

                        // buscar si alguna keybind configurada coincide con los modificadores y la tecla
                        let matched_bind = state.config.parsed_keybinds.iter().find(|bind| {
                            // chequear modificadores
                            if bind.logo != mods.logo || bind.shift != mods.shift || bind.ctrl != mods.ctrl || bind.alt != mods.alt {
                                return false;
                            }

                            // chequear tecla: coincide con la tecla sin shift (raw) o con la tecla modificada
                            raw_syms.contains(&bind.sym) || mod_sym == bind.sym
                        });

                        if let Some(bind) = matched_bind {
                            match &bind.action {
                                KeyAction::Workspace(idx) => {
                                    let idx = *idx;
                                    if idx < state.workspaces.len() && idx != state.active_workspace {
                                        info!("[workspace] cambiar a workspace {}", idx + 1);
                                        state.active_workspace = idx;
                                        state.retile();
                                        // enfocar la primera ventana visible del nuevo workspace (o None si esta vacio)
                                        let first_win = state
                                            .windows()
                                            .iter()
                                            .find(|w| !w.minimized)
                                            .map(|w| w.surface.wl_surface().clone());
                                        state.set_keyboard_focus_surface(first_win.as_ref(), serial);
                                    }
                                }
                                KeyAction::FocusNext => {
                                    let non_minimized: Vec<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> = state
                                        .windows()
                                        .iter()
                                        .filter(|w| !w.minimized)
                                        .map(|w| w.surface.wl_surface().clone())
                                        .collect();

                                    if !non_minimized.is_empty() {
                                        let kb = state.seat.get_keyboard().unwrap();
                                        let current_focus = kb.current_focus();
                                        let next_idx = match current_focus.as_ref() {
                                            Some(curr) => {
                                                let pos = non_minimized.iter().position(|s| s == curr).unwrap_or(0);
                                                (pos + 1) % non_minimized.len()
                                            }
                                            None => 0,
                                        };
                                        let target = &non_minimized[next_idx];
                                        info!("[focus] focus next -> ventana {}", next_idx + 1);
                                        state.set_keyboard_focus_surface(Some(target), serial);
                                    }
                                }
                                KeyAction::FocusPrev => {
                                    let non_minimized: Vec<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> = state
                                        .windows()
                                        .iter()
                                        .filter(|w| !w.minimized)
                                        .map(|w| w.surface.wl_surface().clone())
                                        .collect();

                                    if !non_minimized.is_empty() {
                                        let kb = state.seat.get_keyboard().unwrap();
                                        let current_focus = kb.current_focus();
                                        let prev_idx = match current_focus.as_ref() {
                                            Some(curr) => {
                                                let pos = non_minimized.iter().position(|s| s == curr).unwrap_or(0);
                                                if pos == 0 {
                                                    non_minimized.len() - 1
                                                } else {
                                                    pos - 1
                                                }
                                            }
                                            None => non_minimized.len() - 1,
                                        };
                                        let target = &non_minimized[prev_idx];
                                        info!("[focus] focus prev -> ventana {}", prev_idx + 1);
                                        state.set_keyboard_focus_surface(Some(target), serial);
                                    }
                                }
                                KeyAction::Close => {
                                    let kb = state.seat.get_keyboard().unwrap();
                                    if let Some(focused_surface) = kb.current_focus() {
                                        if let Some(win) = state
                                            .windows()
                                            .iter()
                                            .find(|w| w.surface.wl_surface() == &focused_surface)
                                        {
                                            info!("[close] cerrando ventana {:?}", win.surface.wl_surface().id());
                                            win.surface.send_close();
                                        }
                                    }
                                }
                                KeyAction::MoveToWorkspace(idx) => {
                                    let idx = *idx;
                                    if idx < state.workspaces.len() && idx != state.active_workspace {
                                        let kb = state.seat.get_keyboard().unwrap();
                                        let current_focus = kb.current_focus();

                                        let win_idx = current_focus.as_ref().and_then(|focused| {
                                            state
                                                .windows()
                                                .iter()
                                                .position(|w| w.surface.wl_surface() == focused)
                                        });

                                        if let Some(pos) = win_idx {
                                            info!("[workspace] mover ventana al workspace {}", idx + 1);
                                            let win = state.workspaces[state.active_workspace].windows.remove(pos);
                                            state.workspaces[idx].windows.push(win);
                                            state.retile();

                                            let next_surface = state
                                                .windows()
                                                .iter()
                                                .find(|w| !w.minimized)
                                                .map(|w| w.surface.wl_surface().clone());
                                            state.set_keyboard_focus_surface(next_surface.as_ref(), serial);
                                        }
                                    }
                                }
                                KeyAction::Exit => {
                                    info!("[exit] saliendo de vinland...");
                                    state.loop_signal.stop();
                                }
                                KeyAction::Screenshot => {
                                    info!("[screenshot] solicitud de captura de pantalla completa");
                                    state.pending_screenshot = Some(crate::state::ScreenshotTarget::FullScreen);
                                    state.backend.window().request_redraw();
                                }
                                KeyAction::ScreenshotWindow => {
                                    let kb = state.seat.get_keyboard().unwrap();
                                    let current_focus = kb.current_focus();
                                    let window_rect = current_focus.as_ref().and_then(|focused| {
                                        state
                                            .windows()
                                            .iter()
                                            .find(|w| w.surface.wl_surface() == focused)
                                            .map(|w| w.rect)
                                    });

                                    if let Some(rect) = window_rect {
                                        info!("[screenshot] solicitud de captura de ventana: {:?}", rect);
                                        state.pending_screenshot = Some(crate::state::ScreenshotTarget::Window(rect));
                                    } else {
                                        info!("[screenshot] no hay ventana enfocada, capturando pantalla completa");
                                        state.pending_screenshot = Some(crate::state::ScreenshotTarget::FullScreen);
                                    }
                                    state.backend.window().request_redraw();
                                }
                                KeyAction::Exec(cmd) => {
                                    info!("[exec] ejecutando comando: {}", cmd);
                                    let parts: Vec<&str> = cmd.split_whitespace().collect();
                                    if let Some((program, args)) = parts.split_first() {
                                        let mut command = std::process::Command::new(program);
                                        command.args(args);
                                        command.env("WAYLAND_DISPLAY", "wayland-1");
                                        command.env("DISPLAY", ":1");
                                        if let Err(e) = command.spawn() {
                                            tracing::warn!("[exec] error al ejecutar {:?}: {}", cmd, e);
                                        }
                                    }
                                }
                            }
                            return FilterResult::Intercept(());
                        }

                        FilterResult::Forward
                    },
                );
            }

            // movimiento del mouse (winit usa coordenadas absolutas, no relativas)
            InputEvent::PointerMotionAbsolute { event } => {
                let scale_factor = self.backend.scale_factor();
                let output_size = self
                    .backend
                    .window_size()
                    .to_f64()
                    .to_logical(scale_factor)
                    .to_i32_round();

                // position_transformed -> convierte coordenadas relativas (0..1) a píxeles lógicos
                let pos: Point<f64, Logical> = (
                    event.x_transformed(output_size.w),
                    event.y_transformed(output_size.h),
                ).into();

                self.pointer_pos = pos;

                // si estamos arrastrando una ventana flotante, actualizar su posición en pantalla
                if let crate::state::DragState::FloatMove { ref source_surface, grab_offset } = self.drag_state {
                    let source_surface = source_surface.clone();
                    if let Some(win) = self.windows_mut().iter_mut().find(|w| w.surface.wl_surface() == &source_surface) {
                        win.rect.loc = (pos - grab_offset).to_i32_round();
                        self.backend.window().request_redraw();
                    }
                    return;
                }

                // si estamos redimensionando una ventana flotante
                if let crate::state::DragState::FloatResize {
                    ref source_surface,
                    initial_loc,
                    initial_size,
                    start_pos,
                    edges,
                } = self.drag_state
                {
                    let source_surface = source_surface.clone();
                    let dx = (pos.x - start_pos.x) as i32;
                    let dy = (pos.y - start_pos.y) as i32;

                    use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;

                    let (new_x, new_y, new_w, new_h) = if let Some(edge) = edges {
                        let mut nx = initial_loc.x;
                        let mut ny = initial_loc.y;
                        let mut nw = initial_size.w;
                        let mut nh = initial_size.h;

                        let (is_left, is_right, is_top, is_bottom) = match edge {
                            ResizeEdge::Top => (false, false, true, false),
                            ResizeEdge::Bottom => (false, false, false, true),
                            ResizeEdge::Left => (true, false, false, false),
                            ResizeEdge::Right => (false, true, false, false),
                            ResizeEdge::TopLeft => (true, false, true, false),
                            ResizeEdge::TopRight => (false, true, true, false),
                            ResizeEdge::BottomLeft => (true, false, false, true),
                            ResizeEdge::BottomRight => (false, true, false, true),
                            _ => (false, false, false, false),
                        };

                        if is_left {
                            nx += dx;
                            nw -= dx;
                        } else if is_right {
                            nw += dx;
                        }

                        if is_top {
                            ny += dy;
                            nh -= dy;
                        } else if is_bottom {
                            nh += dy;
                        }

                        if nw < 100 {
                            if is_left {
                                nx -= 100 - nw;
                            }
                            nw = 100;
                        }
                        if nh < 100 {
                            if is_top {
                                ny -= 100 - nh;
                            }
                            nh = 100;
                        }

                        (nx, ny, nw, nh)
                    } else {
                        // resize libre desde la esquina inferior derecha con Super+RightClick
                        let nw = (initial_size.w + dx).max(100);
                        let nh = (initial_size.h + dy).max(100);
                        (initial_loc.x, initial_loc.y, nw, nh)
                    };

                    if let Some(win) = self.windows_mut().iter_mut().find(|w| w.surface.wl_surface() == &source_surface) {
                        win.rect = smithay::utils::Rectangle::new((new_x, new_y).into(), (new_w, new_h).into());
                        win.surface.with_pending_state(|s| {
                            s.size = Some(Size::from((new_w, new_h)));
                        });
                        win.surface.send_configure();
                        self.backend.window().request_redraw();
                    }
                    return;
                }

                // si estamos ajustando el ratio del mosaico en tiempo real con Super+RightClick
                if let crate::state::DragState::TileRatioResize { start_x, initial_ratio } = self.drag_state {
                    let scale_factor = self.backend.scale_factor();
                    let out_w = self.backend.window_size().to_f64().to_logical(scale_factor).w;
                    let dx = pos.x - start_x;
                    let delta_ratio = (dx / out_w) as f32;
                    let new_ratio = (initial_ratio + delta_ratio).clamp(0.1, 0.9);
                    if (self.config.tiling.master_ratio - new_ratio).abs() > 0.005 {
                        self.config.tiling.master_ratio = new_ratio;
                        self.retile();
                        self.backend.window().request_redraw();
                    }
                    return;
                }

                // focus-follows-pointer: actualizamos el foco de teclado y el estado Activated
                // al mover el mouse, no solo al hacer click. Esto garantiza que cuando el cursor
                // entra a una ventana, la ventana ya está Activated ANTES de que llegue cualquier click.
                // Sin esto, GTK recibe el primer click con la ventana inactiva y deshabilita
                // los GAction-widgets (como el botón ≡), ignorando el click.
                let serial = SERIAL_COUNTER.next_serial();
                self.update_keyboard_focus(pos, serial);

                let pointer = self.seat.get_pointer().unwrap();

                // SIEMPRE pasamos el focus real — incluso con un grab activo.
                // PopupPointerGrab necesita surface_under para saber si el cursor
                // está dentro o fuera del popup, y así decidir si redirigir o dismiss.
                // Pasar None lo dejaba ciego y el menú ≡ nunca se abría.
                let focus = self.surface_under(pos);

                pointer.motion(
                    self,
                    focus,
                    &MotionEvent {
                        location: pos,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: Event::time_msec(&event),
                    },
                );
                pointer.frame(self);
            }

            // click de mouse
            InputEvent::PointerButton { event } => {
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state  = wl_pointer::ButtonState::from(event.state());
                let target = self.surface_under(self.pointer_pos);

                info!("[CLICK] pos={:?} button={} state={:?} target={:?}",
                    self.pointer_pos, button, state, target.as_ref().map(|(s, _)| s.id()));

                if wl_pointer::ButtonState::Pressed == state {
                    self.last_pointer_serial = Some(serial);

                    // si super está presionado y es click izquierdo (272), iniciar drag/move
                    if self.super_pressed && button == 272 {
                        let active_ws = self.active_workspace;
                        let found = self.workspaces[active_ws].windows.iter().rev().find(|w| {
                            if w.minimized {
                                return false;
                            }
                            if let Some((ref s, _)) = target {
                                if w.surface.wl_surface() == s {
                                    return true;
                                }
                                let mut in_tree = false;
                                smithay::wayland::compositor::with_surface_tree_downward(
                                    w.surface.wl_surface(),
                                    (),
                                    |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                                    |child, _, _| {
                                        if child == s {
                                            in_tree = true;
                                        }
                                    },
                                    |_, _, _| true,
                                );
                                if in_tree {
                                    return true;
                                }
                            }
                            w.rect.to_f64().contains(self.pointer_pos)
                        }).map(|w| (w.surface.wl_surface().clone(), w.floating, w.rect));

                        if let Some((surf, floating, rect)) = found {
                            self.set_keyboard_focus_surface(Some(&surf), serial);
                            if floating {
                                let grab_offset = self.pointer_pos - rect.loc.to_f64();
                                self.drag_state = crate::state::DragState::FloatMove { source_surface: surf, grab_offset };
                                info!("[drag] iniciado movimiento de ventana flotante con Super+Click");
                            } else {
                                self.drag_state = crate::state::DragState::TileSwap { source_surface: surf, start_pos: self.pointer_pos };
                                info!("[drag] iniciado swap de ventana tileada con Super+Click");
                            }
                            return;
                        }
                    }

                    // si super está presionado y es click derecho (273), iniciar resize
                    if self.super_pressed && button == 273 {
                        let active_ws = self.active_workspace;
                        let found = self.workspaces[active_ws].windows.iter().rev().find(|w| {
                            if w.minimized {
                                return false;
                            }
                            if let Some((ref s, _)) = target {
                                if w.surface.wl_surface() == s {
                                    return true;
                                }
                                let mut in_tree = false;
                                smithay::wayland::compositor::with_surface_tree_downward(
                                    w.surface.wl_surface(),
                                    (),
                                    |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                                    |child, _, _| {
                                        if child == s {
                                            in_tree = true;
                                        }
                                    },
                                    |_, _, _| true,
                                );
                                if in_tree {
                                    return true;
                                }
                            }
                            w.rect.to_f64().contains(self.pointer_pos)
                        }).map(|w| (w.surface.wl_surface().clone(), w.floating, w.rect));

                        if let Some((surf, floating, rect)) = found {
                            self.set_keyboard_focus_surface(Some(&surf), serial);
                            if floating {
                                self.drag_state = crate::state::DragState::FloatResize {
                                    source_surface: surf,
                                    initial_loc: rect.loc,
                                    initial_size: rect.size,
                                    start_pos: self.pointer_pos,
                                    edges: None,
                                };
                                info!("[drag] iniciado resize de ventana flotante con Super+RightClick");
                            } else {
                                self.drag_state = crate::state::DragState::TileRatioResize {
                                    start_x: self.pointer_pos.x,
                                    initial_ratio: self.config.tiling.master_ratio,
                                };
                                info!("[drag] iniciado resize de master_ratio con Super+RightClick");
                            }
                            return;
                        }
                    }

                    // al hacer click normal, actualizamos el foco del teclado SOLO si no hay un grab activo (ej: popup/menú)
                    let keyboard = self.seat.get_keyboard().unwrap();
                    if !keyboard.is_grabbed() {
                        self.update_keyboard_focus(self.pointer_pos, serial);
                    }
                } else if wl_pointer::ButtonState::Released == state && (button == 272 || button == 273) {
                    match &self.drag_state {
                        crate::state::DragState::TileSwap { source_surface, .. } => {
                            let source_surf = source_surface.clone();
                            let active_ws = self.active_workspace;
                            let target_surf = self.workspaces[active_ws].windows.iter().rev().find(|w| {
                                !w.minimized && !w.floating && w.surface.wl_surface() != &source_surf && w.rect.to_f64().contains(self.pointer_pos)
                            }).map(|w| w.surface.wl_surface().clone());

                            if let Some(target_s) = target_surf {
                                let src_pos = self.workspaces[active_ws].windows.iter().position(|w| w.surface.wl_surface() == &source_surf);
                                let dst_pos = self.workspaces[active_ws].windows.iter().position(|w| w.surface.wl_surface() == &target_s);
                                if let (Some(si), Some(di)) = (src_pos, dst_pos) {
                                    let src_order = self.workspaces[active_ws].windows[si].tile_order;
                                    let dst_order = self.workspaces[active_ws].windows[di].tile_order;
                                    self.workspaces[active_ws].windows[si].tile_order = dst_order;
                                    self.workspaces[active_ws].windows[di].tile_order = src_order;
                                    self.retile();
                                    self.backend.window().request_redraw();
                                    info!("[tiling] swap completado con Super+Click entre ventanas");
                                }
                            }
                            self.drag_state = crate::state::DragState::None;
                            return;
                        }
                        crate::state::DragState::FloatMove { .. } => {
                            self.drag_state = crate::state::DragState::None;
                            info!("[drag] finalizado movimiento de ventana flotante");
                            return;
                        }
                        crate::state::DragState::FloatResize { .. } => {
                            self.drag_state = crate::state::DragState::None;
                            info!("[drag] finalizado resize de ventana flotante");
                            return;
                        }
                        crate::state::DragState::TileRatioResize { .. } => {
                            self.drag_state = crate::state::DragState::None;
                            info!("[drag] finalizado ajuste de master_ratio");
                            return;
                        }
                        crate::state::DragState::None => {}
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                // Siempre mandamos motion antes de button (incluso con grab activo).
                // PopupPointerGrab.button() usa current_location() para saber si el
                // click es dentro o fuera del popup → necesita posición actualizada.
                let focus = self.surface_under(self.pointer_pos);
                pointer.motion(
                    self,
                    focus,
                    &MotionEvent {
                        location: self.pointer_pos,
                        serial,
                        time: Event::time_msec(&event),
                    },
                );

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: state.try_into().unwrap(),
                        serial,
                        time: Event::time_msec(&event),
                    },
                );
                pointer.frame(self);
            }

            // scroll
            InputEvent::PointerAxis { event } => {
                let h = event.amount(input::Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_v120(input::Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0);
                let v = event.amount(input::Axis::Vertical)
                    .unwrap_or_else(|| event.amount_v120(input::Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0);

                let mut frame = AxisFrame::new(Event::time_msec(&event)).source(event.source());
                if h != 0.0 {
                    frame = frame.value(Axis::Horizontal, h);
                    if let Some(d) = event.amount_v120(input::Axis::Horizontal) {
                        frame = frame.v120(Axis::Horizontal, d as i32);
                    }
                    if event.source() == AxisSource::Finger {
                        frame = frame.stop(Axis::Horizontal);
                    }
                }
                if v != 0.0 {
                    frame = frame.value(Axis::Vertical, v);
                    if let Some(d) = event.amount_v120(input::Axis::Vertical) {
                        frame = frame.v120(Axis::Vertical, d as i32);
                    }
                    if event.source() == AxisSource::Finger {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }

            _ => {} // resto de eventos (touch, tablet, etc.) ignorados por ahora
        }
    }

    // surface_under -> devuelve qué wl_surface está bajo un punto lógico
    // itera respetando el Z-order: Overlay > Popups/Top > Ventanas > Bottom > Background
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output_size = self
            .output
            .current_mode()
            .map(|m| m.size.to_logical(1))
            .unwrap_or_else(|| Size::from((1920, 1080)));

        // Helper para testear una layer surface
        let check_layer = |target_layer: Layer| {
            for item in self.layer_surfaces.iter().rev() {
                if !item.surface.alive() || item.layer != target_layer {
                    continue;
                }
                if let Some(rect) = layer_surface_geometry(&item.surface, output_size) {
                    let local = pos - rect.loc.to_f64();
                    if let Some((subsurface, sub_offset)) = under_from_surface_tree(
                        item.surface.wl_surface(),
                        local,
                        (0, 0),
                        WindowSurfaceType::ALL,
                    ) {
                        let global_pos = rect.loc.to_f64() + sub_offset.to_f64();
                        return Some((subsurface, global_pos));
                    }
                }
            }
            None
        };

        // 1. Capa Overlay (pantalla completa / lockscreens / avisos urgentes)
        if let Some(hit) = check_layer(Layer::Overlay) {
            return Some(hit);
        }

        // 2. Popups XDG y Capa Top (barras, paneles)
        if let Some(hit) = check_layer(Layer::Top) {
            return Some(hit);
        }

        // 3. Ventanas Toplevel y sus Popups
        for window in self.windows().iter().rev() {
            if window.minimized {
                continue;
            }

            // Popups de esta ventana
            for (popup, popup_location) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
                if !popup.alive() {
                    continue;
                }
                let popup_geo_loc = popup.geometry().loc;
                let global_popup_loc = window.rect.loc + popup_location - popup_geo_loc;
                let local = pos - global_popup_loc.to_f64();
                if let Some((subsurface, sub_offset)) = under_from_surface_tree(
                    popup.wl_surface(),
                    local,
                    (0, 0),
                    WindowSurfaceType::ALL,
                ) {
                    let global_pos = global_popup_loc.to_f64() + sub_offset.to_f64();
                    return Some((subsurface, global_pos));
                }
            }

            let geo_loc = smithay::wayland::compositor::with_states(window.surface.wl_surface(), |states| {
                states
                    .cached_state
                    .get::<smithay::wayland::shell::xdg::SurfaceCachedState>()
                    .current()
                    .geometry
            })
            .unwrap_or_default()
            .loc;

            let window_loc = window.rect.loc - geo_loc;

            // La ventana misma
            let local = pos - window_loc.to_f64();
            if let Some((subsurface, sub_offset)) = under_from_surface_tree(
                window.surface.wl_surface(),
                local,
                (0, 0),
                WindowSurfaceType::ALL,
            ) {
                let global_pos = window_loc.to_f64() + sub_offset.to_f64();
                return Some((subsurface, global_pos));
            }
        }

        // 4. Capa Bottom (widgets de escritorio)
        if let Some(hit) = check_layer(Layer::Bottom) {
            return Some(hit);
        }

        // 5. Capa Background (fondos animados / wallpaper manager)
        if let Some(hit) = check_layer(Layer::Background) {
            return Some(hit);
        }

        None
    }

    // set_keyboard_focus_surface -> asigna el foco del teclado a una superficie especifica
    // y actualiza el estado Activated en todas las ventanas del workspace
    pub fn set_keyboard_focus_surface(
        &mut self,
        target_surface: Option<&WlSurface>,
        serial: smithay::utils::Serial,
    ) {
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.is_grabbed() {
            return;
        }

        for window in self.windows_mut() {
            let is_focused = target_surface
                .map(|s| {
                    // 1. es la superficie principal o subsuperficie de ella
                    let mut found = false;
                    smithay::wayland::compositor::with_surface_tree_downward(
                        window.surface.wl_surface(),
                        (),
                        |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                        |child, _, _| {
                            if child == s {
                                found = true;
                            }
                        },
                        |_, _, _| true,
                    );
                    if found {
                        return true;
                    }

                    // 2. es un popup de esta ventana (o subsuperficie de un popup)
                    for (popup, _) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
                        if !popup.alive() {
                            continue;
                        }
                        let mut popup_found = false;
                        smithay::wayland::compositor::with_surface_tree_downward(
                            popup.wl_surface(),
                            (),
                            |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                            |child, _, _| {
                                if child == s {
                                    popup_found = true;
                                }
                            },
                            |_, _, _| true,
                        );
                        if popup_found {
                            return true;
                        }
                    }

                    false
                })
                .unwrap_or(false);

            let mut changed = false;
            window.surface.with_pending_state(|state| {
                if is_focused {
                    if !state.states.contains(xdg_toplevel::State::Activated) {
                        state.states.set(xdg_toplevel::State::Activated);
                        changed = true;
                    }
                } else {
                    if state.states.contains(xdg_toplevel::State::Activated) {
                        state.states.unset(xdg_toplevel::State::Activated);
                        changed = true;
                    }
                }
            });

            if changed {
                window.surface.send_configure();
            }
        }

        // Reordenar en Z-stack: mover la ventana que contiene la superficie enfocada al final
        // para que se renderice encima y quede al frente en la jerarquia visual
        if let Some(target) = target_surface {
            let active_ws = self.active_workspace;
            let target_pos = self.workspaces[active_ws].windows.iter().position(|w| {
                if w.surface.wl_surface() == target {
                    return true;
                }
                let mut found = false;
                smithay::wayland::compositor::with_surface_tree_downward(
                    w.surface.wl_surface(),
                    (),
                    |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                    |child, _, _| {
                        if child == target {
                            found = true;
                        }
                    },
                    |_, _, _| true,
                );
                if found {
                    return true;
                }
                for (popup, _) in PopupManager::popups_for_surface(w.surface.wl_surface()) {
                    if !popup.alive() {
                        continue;
                    }
                    let mut popup_found = false;
                    smithay::wayland::compositor::with_surface_tree_downward(
                        popup.wl_surface(),
                        (),
                        |_, _, _| smithay::wayland::compositor::TraversalAction::DoChildren(()),
                        |child, _, _| {
                            if child == target {
                                popup_found = true;
                            }
                        },
                        |_, _, _| true,
                    );
                    if popup_found {
                        return true;
                    }
                }
                false
            });

            if let Some(pos) = target_pos {
                let last_idx = self.workspaces[active_ws].windows.len().saturating_sub(1);
                if pos < last_idx {
                    let win = self.workspaces[active_ws].windows.remove(pos);
                    self.workspaces[active_ws].windows.push(win);
                }
            }
        }

        keyboard.set_focus(self, target_surface.cloned(), serial);

        // forzar un redraw para que el cambio de color del borde activo/inactivo
        // se refleje en el siguiente frame sin esperar un evento de movimiento del cursor
        self.backend.window().request_redraw();
    }

    // update_keyboard_focus -> busca la superficie bajo las coordenadas del mouse y le da foco
    pub fn update_keyboard_focus(&mut self, pos: Point<f64, Logical>, serial: smithay::utils::Serial) {
        let target_surface = self.surface_under(pos).map(|(s, _)| s);
        self.set_keyboard_focus_surface(target_surface.as_ref(), serial);
    }
}
