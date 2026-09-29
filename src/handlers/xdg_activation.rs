// xdg_activation.rs
// Manejo del protocolo xdg_activation_v1
// Permite la transferencia de foco entre aplicaciones (ej. un link abierto desde el terminal o Discord)
// evitando el "focus stealing" no deseado.

use crate::state::Vinland;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

impl XdgActivationHandler for Vinland {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, _data: XdgActivationTokenData) -> bool {
        true
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        // Validar que el token no haya expirado (ej. dentro de los últimos 10 segundos)
        if token_data.timestamp.elapsed().as_secs() > 10 {
            tracing::warn!("[xdg_activation] token expirado, ignorando solicitud de activación");
            return;
        }

        tracing::info!("[xdg_activation] solicitud de activación concedida para app_id {:?}", token_data.app_id);

        // Si la superficie pertenece a una ventana en otro workspace, cambiar al workspace correspondiente
        let target_ws = self.workspaces.iter().position(|ws| {
            ws.windows.iter().any(|w| w.surface.wl_surface() == &surface)
        });

        if let Some(idx) = target_ws {
            if idx != self.active_workspace {
                self.switch_workspace(idx);
            }
        }

        let serial = SERIAL_COUNTER.next_serial();
        self.set_keyboard_focus_surface(Some(&surface), serial);
        self.backend.window().request_redraw();
    }
}
