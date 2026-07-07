// state.rs
// struct central de vinland y constructor

use smithay::wayland::compositor::{CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::{XdgShellState, ToplevelSurface};
use smithay::input::{SeatState, Seat, keyboard::XkbConfig, pointer::CursorImageStatus};
use smithay::output::{Output, PhysicalProperties, Subpixel, Mode, Scale as OutputScale};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::WinitGraphicsBackend;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Point, Logical, Transform, Rectangle, Size};
use calloop::LoopSignal;

// window -> representa una ventana y su posición/tamaño en pantalla (tiling layout)
// tener surface y rect juntos = mejor cache locality que tenerlos en vecs separados
pub struct Window {
    pub surface: ToplevelSurface,
    pub rect:    Rectangle<i32, Logical>,
}

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
    pub windows:           Vec<Window>,          // ventanas activas del compositor
    pub pointer_pos:       Point<f64, Logical>, // posición actual del cursor en espacio lógico
    pub data_device_state: DataDeviceState,
    pub cursor_status:     CursorImageStatus,   // estado/imagen actual del cursor
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
        let data_device_state = DataDeviceState::new::<Vinland>(&display_handle);

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
            pointer_pos: (0.0, 0.0).into(),
            data_device_state,
            cursor_status: CursorImageStatus::default_named(),
        };

        (state, winit_evt_loop)
    }

    // retile -> calcula y envía la nueva disposición tiling a todas las ventanas
    // layout: 1 ventana = fullscreen, 2+ = master izquierda + stack derecha apilado
    pub fn retile(&mut self) {
        let n = self.windows.len();
        if n == 0 { return; }

        // tamaño total de la pantalla en coordenadas lógicas (scale=1 → lógico == físico)
        let out_size = self.output.current_mode()
            .map(|m| m.size)
            .unwrap_or_else(|| (1920, 1080).into());
        let w = out_size.w;
        let h = out_size.h;

        if n == 1 {
            // única ventana: fullscreen
            let rect = Rectangle::new((0, 0).into(), (w, h).into());
            self.windows[0].rect = rect;
            self.windows[0].surface.with_pending_state(|s| {
                s.size = Some(Size::from((w, h)));
            });
            self.windows[0].surface.send_pending_configure();
        } else {
            // master: mitad izquierda, ocupa toda la altura
            let master_w = w / 2;
            self.windows[0].rect = Rectangle::new((0, 0).into(), (master_w, h).into());
            self.windows[0].surface.with_pending_state(|s| {
                s.size = Some(Size::from((master_w, h)));
            });
            self.windows[0].surface.send_pending_configure();

            // stack: mitad derecha, dividida en franjas horizontales iguales
            let stack_w = w - master_w;
            let stack_h = h / (n as i32 - 1);
            for (i, win) in self.windows[1..].iter_mut().enumerate() {
                let y = i as i32 * stack_h;
                win.rect = Rectangle::new((master_w, y).into(), (stack_w, stack_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((stack_w, stack_h)));
                });
                win.surface.send_pending_configure();
            }
        }
    }
}

