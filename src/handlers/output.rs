// output handler -> wl_output (notificacion de cambios en pantalla)

use smithay::wayland::output::OutputHandler;

use crate::state::Vinland;

// output handler no tiene métodos obligatorios para implementar —
// smithay solo necesita que la implementación exista para registrar
// el delegate y poder crear globales wl_output.
impl OutputHandler for Vinland {}
