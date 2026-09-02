// traits de un protocolo Wayland distinto.

pub mod compositor; // wl_compositor, wl_shm, wl_buffer
pub mod layer_shell; // zwlr_layer_shell_v1
pub mod output; // wl_output
pub mod seat; // wl_seat, wl_keyboard, wl_pointer
pub mod selection; // wl_data_device_manager
pub mod screencopy; // zwlr_screencopy_manager_v1
pub mod xdg_decoration; // zxdg_decoration_manager_v1
pub mod xdg_foreign; // xdg_foreign, xdg_foreign_toplevel
pub mod xdg_shell; // xdg_wm_base, xdg_surface, xdg_toplevel
pub mod xwm; // xwayland y x11wm


