// traits de un protocolo Wayland distinto.

pub mod compositor; // wl_compositor, wl_shm, wl_buffer
pub mod output; // wl_output
pub mod seat; // wl_seat, wl_keyboard, wl_pointer
pub mod selection; // wl_data_device_manager
pub mod xdg_foreign; // xdg_foreign, xdg_foreign_toplevel
pub mod xdg_shell; // xdg_wm_base, xdg_surface, xdg_toplevel
