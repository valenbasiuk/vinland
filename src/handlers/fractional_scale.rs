// handlers/fractional_scale.rs
// implementacion del protocolo wp_fractional_scale_v1
// informa a las aplicaciones el factor de escala exacto para renderizado HiDPI

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::fractional_scale::FractionalScaleHandler;

use crate::state::Vinland;

impl FractionalScaleHandler for Vinland {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.backend.scale_factor();
        smithay::wayland::compositor::with_states(&surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |fractional_scale| {
                fractional_scale.set_preferred_scale(scale);
            });
        });
    }
}
