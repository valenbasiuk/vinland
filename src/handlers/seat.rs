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
use smithay::utils::{Point, Logical, SERIAL_COUNTER};


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
                let keyboard = self.seat.get_keyboard().unwrap();

                // input() -> despacha el evento al cliente con foco actual
                // el closure decide si interceptar la tecla (Intercept) o pasarla (Forward)
                // por ahora siempre Forward: el compositor no tiene atajos propios aún
                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
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

                // al hacer click, actualizamos el foco del teclado SOLO si no hay un grab activo (ej: popup/menú)
                let keyboard = self.seat.get_keyboard().unwrap();
                if wl_pointer::ButtonState::Pressed == state && !keyboard.is_grabbed() {
                    self.update_keyboard_focus(self.pointer_pos, serial);
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
        for window in self.windows.iter().rev() {
            if window.minimized {
                continue;
            }

            // Popups de esta ventana
            for (popup, popup_location) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
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

            // La ventana misma
            let local = pos - window.rect.loc.to_f64();
            if let Some((subsurface, sub_offset)) = under_from_surface_tree(
                window.surface.wl_surface(),
                local,
                (0, 0),
                WindowSurfaceType::ALL,
            ) {
                let global_pos = window.rect.loc.to_f64() + sub_offset.to_f64();
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

    // update_keyboard_focus -> le dice al keyboard handle qué superficie tiene foco
    // y envía el estado State::Activated a la superficie xdg_toplevel para activar CSD y menús GTK
    pub fn update_keyboard_focus(&mut self, pos: Point<f64, Logical>, serial: smithay::utils::Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.is_grabbed() {
            return;
        }

        let target_surface = self.surface_under(pos).map(|(s, _)| s);

        for window in &mut self.windows {
            let is_focused = target_surface
                .as_ref()
                .map(|s| {
                    // 1. Es la superficie principal o subsuperficie de ella
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

                    // 2. Es un popup de esta ventana (o subsuperficie de un popup)
                    for (popup, _) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
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

        keyboard.set_focus(self, target_surface, serial);
    }
}
