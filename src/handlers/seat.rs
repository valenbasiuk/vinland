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


use crate::state::Vinland;

use smithay::reexports::wayland_server::Resource;
use smithay::desktop::utils::under_from_surface_tree;
use smithay::desktop::WindowSurfaceType;

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
                let output_size = self.backend.window_size();

                // position_transformed -> convierte coordenadas relativas (0..1) a píxeles lógicos
                let pos: Point<f64, Logical> = (
                    event.x_transformed(output_size.w),
                    event.y_transformed(output_size.h),
                ).into();

                self.pointer_pos = pos;

                let pointer = self.seat.get_pointer().unwrap();

                // surface_under -> busca qué superficie del cliente está bajo el cursor
                // por ahora: si hay ventanas, la primera recibe el foco del puntero
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

                // al hacer click, actualizamos el foco del teclado a la ventana clickeada
                if wl_pointer::ButtonState::Pressed == state {
                    self.update_keyboard_focus(self.pointer_pos, serial);
                }

                let pointer = self.seat.get_pointer().unwrap();
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
    // itera las ventanas en orden inverso (la última = la más al frente)
    // devuelve la sub-superficie exacta + su posición GLOBAL en pantalla
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        for window in self.windows.iter().rev() {
            if window.rect.to_f64().contains(pos) {
                // pos relativa al origen de la ventana (para buscar en el árbol de sub-superficies)
                let local = pos - window.rect.loc.to_f64();

                // under_from_surface_tree busca la sub-superficie exacta bajo local
                // y devuelve su offset relativo a la ventana raíz
                if let Some((subsurface, sub_offset)) = under_from_surface_tree(
                    window.surface.wl_surface(),
                    local,
                    (0, 0),
                    WindowSurfaceType::ALL,
                ) {
                    // la posición global de la sub-superficie =
                    // origen de la ventana en pantalla + offset dentro de la ventana
                    let global_pos = window.rect.loc.to_f64() + sub_offset.to_f64();
                    return Some((subsurface, global_pos));
                }
            }
        }
        None
    }

    // update_keyboard_focus -> le dice al keyboard handle qué superficie tiene foco
    pub fn update_keyboard_focus(&mut self, pos: Point<f64, Logical>, serial: smithay::utils::Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();
        match self.surface_under(pos) {
            Some((surface, _)) => {
                keyboard.set_focus(self, Some(surface), serial);
            }
            None => {
                keyboard.set_focus(self, None, serial);
            }
        }
    }
}
