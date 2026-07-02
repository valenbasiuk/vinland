use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use calloop::LoopSignal;
use smithay::wayland::shell::xdg::{XdgShellHandler, XdgShellState, ToplevelSurface};
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::Serial;
use smithay::wayland::shell::xdg::PopupSurface;
use smithay::wayland::shell::xdg::PositionerState;


pub struct Vinland {
    pub display_handle: smithay::reexports::wayland_server::DisplayHandle,
    pub compositor_state: CompositorState,
    pub shm_state: smithay::wayland::shm::ShmState,
    pub backend: WinitGraphicsBackend<GlesRenderer>,
    pub loop_signal: LoopSignal,
    pub xdg_shell_state: XdgShellState,
    pub windows: Vec<ToplevelSurface>
}

impl ShmHandler for Vinland {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm_state
    }   // pixel buffer protocol 
}

impl BufferHandler for Vinland {
    fn buffer_destroyed(&mut self, buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer) {
        // All renderers can handle buffer destruction at this point.
        // Some parts of window management may also use this function.
    }
}

    // data de los clientes para el handler
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl smithay::reexports::wayland_server::backend::ClientData for ClientState {}

impl CompositorHandler for Vinland {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {}
}

impl XdgShellHandler for Vinland {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }
        fn new_toplevel(&mut self, surface: ToplevelSurface) {
                    surface.send_configure();
        self.windows.push(surface);
    }

    //windows deletion method
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows.retain(|w| w.wl_surface() != surface.wl_surface());
}
//popups implementation (ventanas para menus / apps que tienen padre)
            fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
    fn reposition_request(&mut self, _surface: PopupSurface, _positioner: PositionerState, _token: u32) {}

}


//Delegados de las interfaces
smithay::delegate_dispatch2!(Vinland);
