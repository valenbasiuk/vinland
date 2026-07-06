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

// seathandler -> define qué tipos reciben foco de cada dispositivo
impl SeatHandler for Vinland {
    type KeyboardFocus = WlSurface;
    type PointerFocus  = WlSurface;
    type TouchFocus    = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Vinland> {
        &mut self.seat_state
    }

    // llamado cuando el foco del teclado cambia entre superficies
    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

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
    // también devuelve el offset de esa superficie dentro del espacio global
    // todo: cuando implementes tiles/stacking, acá vas a iterar en z-order
    //       y vas a usar _pos para detectar qué ventana está bajo el cursor
    pub fn surface_under(
        &self,
        _pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        // por ahora: la última ventana en el vec (la más reciente) recibe el foco
        // todas las ventanas están posicionadas en (0,0)
        self.windows.last().map(|w| (w.wl_surface().clone(), (0.0, 0.0).into()))
    }

    // update_keyboard_focus -> le dice al keyboard handle qué superficie tiene foco
    // se llama al hacer click, al abrir ventana nueva, etc.
    pub fn update_keyboard_focus(&mut self, pos: Point<f64, Logical>, serial: smithay::utils::Serial) {
        let keyboard = self.seat.get_keyboard().unwrap();

        // si el pointer no está en ninguna superficie, quitamos el foco del teclado
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
