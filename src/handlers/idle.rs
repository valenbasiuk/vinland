// idle.rs
// Manejo del protocolo ext-idle-notify-v1
// Permite a herramientas como swayidle detectar periodos de inactividad
// y activar el apagado de pantalla o bloqueo.

use crate::state::Vinland;
use smithay::wayland::idle_notify::{IdleNotifierHandler, IdleNotifierState};

impl IdleNotifierHandler for Vinland {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.idle_notifier_state
    }
}
