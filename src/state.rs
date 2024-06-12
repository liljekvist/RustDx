use std::iter;
use std::time::Instant;
use imgui::{Condition, FontSource};
use imgui_wgpu::{Renderer, RendererConfig};
use wgpu::util::DeviceExt;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;
use cgmath::prelude::*;
use wgpu::{CommandEncoder, StoreOp, TextureView};

use crate::structs::texture::Texture;
use crate::structs::camera::Camera;
use crate::controllers::camera_controller::CameraController;
use crate::structs::vertex::Vertex;
use crate::structs::camera_uniform::CameraUniform;
use crate::structs::instance::Instance;
use crate::structs::instance::InstanceRaw;

const VERTICES: &[Vertex] = &[
    // cube inverted
    Vertex { position: [ 0.5, -0.5,  0.5 ], tex_coords: [1.0, 0.0] }, // 0
    Vertex { position: [ 0.5,  0.5,  0.5 ], tex_coords: [1.0, 1.0] }, // 1
    Vertex { position: [-0.5,  0.5,  0.5 ], tex_coords: [0.0, 1.0] }, // 2
    Vertex { position: [-0.5, -0.5,  0.5 ], tex_coords: [0.0, 0.0] }, // 3
    Vertex { position: [ 0.5, -0.5, -0.5 ], tex_coords: [0.0, 0.0] }, // 4
    Vertex { position: [ 0.5,  0.5, -0.5 ], tex_coords: [0.0, 1.0] }, // 5
    Vertex { position: [-0.5,  0.5, -0.5 ], tex_coords: [1.0, 1.0] }, // 6
    Vertex { position: [-0.5, -0.5, -0.5 ], tex_coords: [1.0, 0.0] }, // 7
];


const INDICES: &[u16] = &[
    // cube inverted
    0, 1, 2, 2, 3, 0, // front
    1, 5, 6, 6, 2, 1, // right
    7, 6, 5, 5, 4, 7, // back
    4, 0, 3, 3, 7, 4, // left
    4, 5, 1, 1, 0, 4, // bottom
    3, 2, 6, 6, 7, 3, // top
];

pub struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    color: wgpu::Color,
    // camera stuff
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_controller: CameraController,
    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,
    // texture stuff
    #[allow(dead_code)]
    diffuse_texture: Texture,
    pub diffuse_bind_group: wgpu::BindGroup,
    pub window: &'a Window,
    depth_texture: Texture,
    // imgui stuff
    imgui: imgui::Context,
    platform: imgui_winit_support::WinitPlatform,
    imgui_renderer: Renderer,
    last_frame: Instant,
    last_cursor: Option<imgui::MouseCursor>,
    show_overlay: bool,
}

impl<'a> State<'a> {
    pub async fn new(window: &'a Window) -> State<'a> {
        let size = window.inner_size();
        const NUM_INSTANCES_PER_ROW: u32 = 10;
        const INSTANCE_DISPLACEMENT: cgmath::Vector3<f32> = cgmath::Vector3::new(NUM_INSTANCES_PER_ROW as f32 * 0.5, 0.0, NUM_INSTANCES_PER_ROW as f32 * 0.5);


        // The instance is a handle to our GPU
        // BackendBit::PRIMARY => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default()
                },
                // Some(&std::path::Path::new("trace")), // Trace path
                None,
            )
            .await
            .unwrap();

        let instances = (0..NUM_INSTANCES_PER_ROW).flat_map(|z| {
            (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                let position = cgmath::Vector3 { x: x as f32, y: 0.0, z: z as f32 } - INSTANCE_DISPLACEMENT;

                let rotation = if position.is_zero() {
                    // this is needed so an object at (0, 0, 0) won't get scaled to zero
                    // as Quaternions can affect scale if they're not created correctly
                    cgmath::Quaternion::from_axis_angle(cgmath::Vector3::unit_z(), cgmath::Deg(0.0))
                } else {
                    cgmath::Quaternion::from_axis_angle(position.normalize(), cgmath::Deg(45.0))
                };

                Instance {
                    position, rotation,
                }
            })
        }).collect::<Vec<_>>();

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&instance_data),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an Srgb surface texture. Using a different
        // one will result all the colors comming out darker. If you want to support non
        // Srgb surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let mut imgui = imgui::Context::create();
        let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
        platform.attach_window(
            imgui.io_mut(),
            &window,
            imgui_winit_support::HiDpiMode::Default,
        );
        imgui.set_ini_filename(None);

        let font_size = (13.0 * window.scale_factor()) as f32;
        imgui.io_mut().font_global_scale = (1.0 / window.scale_factor()) as f32;

        imgui.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

        let renderer_config = RendererConfig {
            texture_format: config.format,
            ..Default::default()
        };

        let imgui_renderer = Renderer::new(&mut imgui, &device, &queue, renderer_config);

        let last_frame = Instant::now();

        let last_cursor = None;

        let depth_texture = Texture::create_depth_texture(&device, &config, "depth_texture");

        let diffuse_bytes = include_bytes!("content/img/grass.png");
        let diffuse_texture =
            Texture::from_bytes(&device, &queue, diffuse_bytes, "happy-tree.png").unwrap();

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let camera = Camera {
            // position the camera 1 unit up and 2 units back
            // +z is out of the screen
            eye: (0.0, 1.0, 2.0).into(),
            // have it look at the origin
            target: (0.0, 0.0, 0.0).into(),
            // which way is "up"
            up: cgmath::Vector3::unit_y(),
            aspect: config.width as f32 / config.height as f32,
            fovy: 45.0,
            znear: 0.1,
            zfar: 100.0,
        };

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        let camera_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Camera Buffer"),
                contents: bytemuck::cast_slice(&[camera_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some("camera_bind_group_layout"),
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }
            ],
            label: Some("camera_bind_group"),
        });



        let shader = device.create_shader_module(wgpu::include_wgsl!("content/shaders/RustDx.wgsl"));

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &texture_bind_group_layout,
                    &camera_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[Vertex::desc(), InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList, // 1.
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // 2.
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less, // 1.
                stencil: wgpu::StencilState::default(), // 2.
                bias: wgpu::DepthBiasState::default(),
            }), // 1.
            multisample: wgpu::MultisampleState {
                count: 1, // 2.
                mask: !0, // 3.
                alpha_to_coverage_enabled: false, // 4.
            },
            multiview: None, // 5.
        });

        let vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(INDICES),
                usage: wgpu::BufferUsages::INDEX,
            }
        );
        let num_indices = INDICES.len() as u32;

        let camera_controller = CameraController::new(0.2);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            depth_texture,
            color: wgpu::Color {
                r: 0.6,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            camera_controller,
            instances,
            instance_buffer,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices,
            diffuse_bind_group,
            diffuse_texture,
            imgui,
            platform,
            imgui_renderer,
            last_frame,
            last_cursor,
            show_overlay: true,
        }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
        self.depth_texture = Texture::create_depth_texture(&self.device, &self.config, "depth_texture");

    }

    #[allow(unused_variables)]
    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event);
        match event {
            WindowEvent::KeyboardInput {
                event:
                KeyEvent {
                    state: ElementState::Pressed,
                    physical_key: PhysicalKey::Code(KeyCode::Space),
                    ..
                }, ..
            } => {
                // Handle keyboard input
                self.show_overlay = !self.show_overlay;
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Handle mouse wheel input
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Handle cursor position input
                self.color = wgpu::Color {
                    r: position.x as f64 / self.size.width as f64,
                    g: position.y as f64 / self.size.height as f64,
                    b: 0.3,
                    a: 1.0,
                };
                true
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Handle mouse button input
                true
            }
            _ => {
                // Ignore all the other events
                false
            }
        }
    }

    pub fn update(&mut self) {
        self.camera_controller.update_camera(&mut self.camera);
        self.camera_uniform.update_view_proj(&self.camera);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
    }


    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(e) => {
                eprintln!("dropped frame: {e:?}");
                return Err(e);
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    // This is what @location(0) in the fragment shader targets
                    Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(
                                self.color
                            ),
                            store: wgpu::StoreOp::Store,
                        }
                    })
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            render_pass.draw_indexed(0..self.num_indices, 0, 0..self.instances.len() as _);
        }

        self.gen_imgui_render_pass(&mut encoder, view);

        self.queue.submit(iter::once(encoder.finish()));
        frame.present();

        Ok(())
    }

    pub fn imgui_handle_event(&mut self, event: &winit::event::Event<()>) {
        self.platform.handle_event(self.imgui.io_mut(), &self.window, event);
    }

    pub fn gen_imgui_render_pass(&mut self, encoder: &mut CommandEncoder, view: TextureView) {
        if self.show_overlay
        {
            let delta_s = self.last_frame.elapsed();
            let now = Instant::now();
            self.imgui.io_mut().update_delta_time(now - self.last_frame);
            self.last_frame = now;

            self.platform
                .prepare_frame(self.imgui.io_mut(), &self.window)
                .expect("Failed to prepare frame");
            let ui = self.imgui.frame();

            {
                let window = ui.window("Debug info");
                let fps = 1.0 / delta_s.as_secs_f64();
                window
                    .size([300.0, 100.0], Condition::FirstUseEver)
                    .position([1.0, 1.0], Condition::FirstUseEver)
                    .build(|| {
                        ui.text(format!(
                            "FPS: {:.1}",
                            fps
                        ));
                        ui.separator();
                        let mouse_pos = ui.io().mouse_pos;
                        ui.text(format!(
                            "Mouse Position: ({:.1},{:.1})",
                            mouse_pos[0], mouse_pos[1]
                        ));
                        ui.text(format!(
                            "Camera Position: ({:.1},{:.1},{:.1})",
                            self.camera.eye[0], self.camera.eye[1], self.camera.eye[2]
                        ));
                    });
            }

            if self.last_cursor != ui.mouse_cursor() {
                self.last_cursor = ui.mouse_cursor();
                self.platform.prepare_render(ui, &self.window);
            }

            let mut imgui_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Imgui Render Pass"),
                color_attachments: &[
                    // This is what @location(0) in the fragment shader targets
                    Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: StoreOp::Discard,
                        }
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.imgui_renderer
                .render(self.imgui.render(), &self.queue, &self.device, &mut imgui_render_pass)
                .expect("Rendering failed");
        }
    }
}