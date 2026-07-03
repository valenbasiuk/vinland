// seat handler -> wl_seat

use smithay::input::{SeatHandler, SeatState, Seat, pointer::CursorImageStatus};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

use crate::state::Vinland;

impl SeatHandler for Vinland {
    // wl_surface es quien recibe foco de teclado/puntero/touch
    type KeyboardFocus = WlSurface;
    type PointerFocus  = WlSurface;
    type TouchFocus    = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Vinland> {
        &mut self.seat_state
    }

    // llamado cuando el foco del teclado cambia de una ventana a otra
    // todo: notificar a la ventana anterior (unfocus) y a la nueva (focus)
    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

    // llamado cuando una app pide cambiar la imagen del cursor
    // todo: renderizar el cursor custom de la app
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}
