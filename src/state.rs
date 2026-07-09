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

        // backend de winit -> renderiza dentro de una ventana del compositor host
        // se inicializa ANTES del output para poder leer el tamaño real de la ventana
        let (backend, winit_evt_loop) = smithay::backend::winit::init::<GlesRenderer>()
            .expect("fallo al inicializar el backend de winit");

        // output: usamos el tamaño real de la ventana Winit para que los clientes
        // no vean un output distinto al espacio que tienen disponible
        let actual_size = backend.window_size();
        let mode = Mode { size: actual_size, refresh: 144000 };
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
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(OutputScale::Integer(1)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.create_global::<Vinland>(&display_handle);

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
    // ignora diálogos o ventanas que tienen padre (transient/floating)
    pub fn retile(&mut self) {
        // contamos solo las ventanas sin padre
        let tiled_count = self.windows.iter().filter(|w| w.surface.parent().is_none()).count();
        if tiled_count == 0 { return; }

        let out_size = self.backend.window_size();
        let w = out_size.w;
        let h = out_size.h;

        let mut tiled_idx = 0;
        let total_tiled = tiled_count;

        for win in self.windows.iter_mut() {
            // si tiene padre, dejamos que flote en su rect actual
            if win.surface.parent().is_some() {
                continue;
            }

            if total_tiled == 1 {
                // única ventana: fullscreen
                win.rect = Rectangle::new((0, 0).into(), (w, h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((w, h)));
                });
                win.surface.send_pending_configure();
            } else if tiled_idx == 0 {
                // master: mitad izquierda
                let master_w = w / 2;
                win.rect = Rectangle::new((0, 0).into(), (master_w, h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((master_w, h)));
                });
                win.surface.send_pending_configure();
                tiled_idx += 1;
            } else {
                // stack: mitad derecha dividida
                let master_w = w / 2;
                let stack_w = w - master_w;
                let stack_h = h / (total_tiled as i32 - 1);
                let y = (tiled_idx - 1) as i32 * stack_h;
                win.rect = Rectangle::new((master_w, y).into(), (stack_w, stack_h).into());
                win.surface.with_pending_state(|s| {
                    s.size = Some(Size::from((stack_w, stack_h)));
                });
                win.surface.send_pending_configure();
                tiled_idx += 1;
            }
        }
    }
}

