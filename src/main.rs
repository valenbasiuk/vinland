use tracing::info;
use tracing_subscriber;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
// struct del estado (state.rs)
mod state;
use state::Vinland;
use state::ClientState;
use smithay::wayland::compositor::CompositorClientState;
use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::Frame;
use smithay::backend::renderer::ImportAll;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::element::{Element, RenderElement, Kind};
use smithay::backend::renderer::element::surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement};
use smithay::utils::{Scale, Point, Physical};
use smithay::input::{SeatState, keyboard::XkbConfig};
use smithay::output::{Output, PhysicalProperties, Subpixel, Mode, Scale as OutputScale};
use smithay::utils::Transform;
use smithay::desktop::utils::send_frames_surface_tree;


fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");
    let start_time = std::time::Instant::now(); //timer para callbaks


    // declara eventloop y display
    let mut event_loop: EventLoop<Vinland> = EventLoop::try_new()
        .expect("fallo al inicializar el event loop");

        // inicio del struc para definir inicializaciones
    let loop_signal = event_loop.get_signal();
    let display: Display<Vinland> = Display::new().unwrap();
    let display_handle = display.handle();
    let compositor_state = smithay::wayland::compositor::CompositorState::new::<Vinland>(&display_handle);
    let shm_state = smithay::wayland::shm::ShmState::new::<Vinland>(&display_handle, vec![]);
    // inicializa el estado de XDG Shell (protocolo de ventanas)
    let xdg_shell_state = smithay::wayland::shell::xdg::XdgShellState::new::<Vinland>(&display_handle);
    // inicializa el seat (protocolo de input: teclado + mouse)
    let mut seat_state = SeatState::new();
    // crea el seat y lo registra como global Wayland
    let mut seat = seat_state.new_wl_seat(&display_handle, "vinland-seat");
    // agrega capacidades: teclado con layout por defecto, delay 200ms, repeat 25Hz
    seat.add_keyboard(XkbConfig::default(), 200, 25).unwrap();


    seat.add_pointer();
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

// pantalla virtual qe anuncia a los clientes
let mode = Mode { size: (1920, 1080).into(), refresh: 60000 };
output.change_current_state(Some(mode), Some(Transform::Normal), Some(OutputScale::Integer(1)), Some((0,0).into()));
output.set_preferred(mode);
output.create_global::<Vinland>(&display_handle);


    let (backend, mut winit_evt_loop) = smithay::backend::winit::init::<smithay::backend::renderer::gles::GlesRenderer>()
        .expect("fallo al inicializar el backend de winit");

    let mut state = Vinland {loop_signal, 
        display_handle, 
        compositor_state, 
        shm_state, 
        backend, 
        windows: Vec::new(),
        xdg_shell_state,
        seat_state,  // el seat ya tiene keyboard y pointer registrados
        seat,
        output,
    };

    info!("display wayland creado");

    //loop handler q conecta display al loop
    let loop_handle = event_loop.handle();
loop_handle
    .insert_source(
        smithay::reexports::calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),


        // event handler — procesa los mensajes entrantes de los clientes
        |_, display, state| {
            // Safety: no se dropea el display
            unsafe {
                display.get_mut().dispatch_clients(state).unwrap();
            }
            Ok(calloop::PostAction::Continue)
        },
    )
    .unwrap();

    info!("display conectado al event loop");

        let socket = smithay::wayland::socket::ListeningSocketSource::new_auto().unwrap();
    let socket_name = socket.socket_name().to_os_string();
    info!("socket wayland creado: {:?}", socket_name);

    loop_handle // para aceptar conexiones entrantes
        .insert_source(socket, |client_stream, _, state| {
            state
                .display_handle
                .insert_client(client_stream, std::sync::Arc::new(ClientState {
                    compositor_state: CompositorClientState::default(),
                }))
                .unwrap();
        })
        .unwrap();

    loop_handle // para manejar eventos de winit
        .insert_source(winit_evt_loop, |event, _, state| {
            match event {
                smithay::backend::winit::WinitEvent::CloseRequested => {
                    info!("ventana cerrada");
                    state.loop_signal.stop();
                }
                smithay::backend::winit::WinitEvent::Redraw => {
                    let size = state.backend.window_size();
                    let damage = smithay::utils::Rectangle::new((0, 0).into(), size);
                    let scale = Scale::from(1.0);

                    // bind: prepara el renderer
                    let (renderer, mut framebuffer) = state.backend.bind().unwrap();

                    // PRIMERO: colectar render elements (necesita &mut renderer, antes del frame)
                    let mut all_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
                    for window in &state.windows {
                        let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                            render_elements_from_surface_tree(
                                renderer,
                                window.wl_surface(),
                                Point::<i32, Physical>::from((0, 0)),
                                scale,
                                1.0,
                                Kind::Unspecified,
                            );
                        all_elements.extend(elems);
                    }

                    // crear el frame (también necesita &mut renderer)
                    let mut frame = renderer.render(
                        &mut framebuffer,
                        size,
                        smithay::utils::Transform::Flipped180,
                    ).unwrap();

                    // fondo verde
                    frame.clear(
                        smithay::backend::renderer::Color32F::from([0.0, 1.0, 0.0, 1.0]),
                        &[damage],
                    ).unwrap();

                    // dibujar cada elemento
                    for element in &all_elements {
                        let _ = element.draw(
                            &mut frame,
                            element.src(),
                            element.geometry(scale),
                            &[damage],
                            &[],
                            None,
                        );
                    }

                    let _ = frame.finish().unwrap();
                    drop(framebuffer);
                    state.backend.submit(None).unwrap();
                          let output = state.output.clone();
                         for window in &state.windows {
                             send_frames_surface_tree(
                                  window.wl_surface(),       // ← la superficie
                                  &output,                   // ← en qué pantalla está
                                start_time.elapsed(),      // ← timestamp actual (Duration)
                                None,                      // ← sin throttle de frames
                                |_, _| Some(output.clone()), // ← closure: todas en nuestro único output
                             );
                         }
                    state.backend.window().request_redraw();
                }
                _ => {}
            }
        })
        .unwrap();

    event_loop.run(None, &mut state, |state| {
        // envia las respuestas pendientes a todos los clientes conectados
        state.display_handle.flush_clients().unwrap();
    }).unwrap();
}