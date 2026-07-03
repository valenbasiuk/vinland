// state.rs
// struct central de vinland y constructor

use smithay::wayland::compositor::{CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmState;
use smithay::wayland::shell::xdg::{XdgShellState, ToplevelSurface};
use smithay::input::{SeatState, Seat, keyboard::XkbConfig};
use smithay::output::{Output, PhysicalProperties, Subpixel, Mode, Scale as OutputScale};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::Transform;
use calloop::LoopSignal;

// vinland -> estado global del compositor
// todos los handlers de protocolos reciben &mut self de este struct
pub struct Vinland {
    pub display_handle:    DisplayHandle,
    pub compositor_state:  CompositorState,
    pub shm_state:         ShmState,
    pub xdg_shell_state:   XdgShellState,
    pub seat_state:        SeatState<Vinland>,
    pub seat:              Seat<Vinland>,       // todo: se usa cuando implementemos input
    pub output:            Output,
    pub backend:           WinitGraphicsBackend<GlesRenderer>,
    pub loop_signal:       LoopSignal,
    pub windows:           Vec<ToplevelSurface>,
}

// clientstate -> datos por cliente conectado
// smithay los almacena internamente y los provee en client_compositor_state()
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl smithay::reexports::wayland_server::backend::ClientData for ClientState {}

// delegate_dispatch2! -> genera el boilerplate de despacho de mensajes wayland
// conecta cada tipo de mensaje wayland al handler correcto en vinland
smithay::delegate_dispatch2!(Vinland);

impl Vinland {
    // new() -> crea e inicializa el compositor completo
    // toma &display para que main.rs pueda moverlo al generic source de calloop
    pub fn new(display: &Display<Vinland>, loop_signal: LoopSignal) -> (Self, impl calloop::EventSource<Event = smithay::backend::winit::WinitEvent, Metadata = (), Ret = ()>) {
        let display_handle = display.handle();

        // protocolos wayland
        let compositor_state = CompositorState::new::<Vinland>(&display_handle);
        let shm_state        = ShmState::new::<Vinland>(&display_handle, vec![]);
        let xdg_shell_state  = XdgShellState::new::<Vinland>(&display_handle);

        // seat (inputs generales)
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&display_handle, "vinland-seat");
        seat.add_keyboard(XkbConfig::default(), 200, 25).unwrap();
        seat.add_pointer();

        // output virtual
        // todo: leer el refresh rate real del monitor físico
        let output = Output::new(
            "vinland-output".into(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Vinland".into(),
                model: "Virtual".into(),
                serial_number: "".into(),
            },
        );
        let mode = Mode { size: (1920, 1080).into(), refresh: 60000 };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(OutputScale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.create_global::<Vinland>(&display_handle);

        // backend de winit -> renderiza dentro de una ventana del compositor host
        let (backend, winit_evt_loop) = smithay::backend::winit::init::<GlesRenderer>()
            .expect("fallo al inicializar el backend de winit");

        let state = Vinland {
            display_handle,
            compositor_state,
            shm_state,
            xdg_shell_state,
            seat_state,
            seat,
            output,
            backend,
            loop_signal,
            windows: Vec::new(),
        };

        (state, winit_evt_loop)
    }
}
