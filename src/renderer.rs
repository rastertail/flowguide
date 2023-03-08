use std::borrow::Cow;

use anyhow::{Context, Result};
use glam::{vec3, Mat4, Vec3};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle, WebDisplayHandle,
    WebWindowHandle,
};
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

use crate::mesh::InputMesh;

struct CanvasWindow {
    window_handle: RawWindowHandle,
    display_handle: RawDisplayHandle,
}

impl CanvasWindow {
    fn new(canvas: &web_sys::HtmlCanvasElement) -> Self {
        let mut web_window = WebWindowHandle::empty();
        web_window.id = canvas
            .dataset()
            .get("rawHandle")
            .expect("Canvas element missing data-raw-handle")
            .parse()
            .expect("data-raw-handle not an integer");
        let window_handle = RawWindowHandle::Web(web_window);

        let web_display = WebDisplayHandle::empty();
        let display_handle = RawDisplayHandle::Web(web_display);

        Self {
            window_handle,
            display_handle,
        }
    }
}

unsafe impl HasRawWindowHandle for CanvasWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.window_handle
    }
}

unsafe impl HasRawDisplayHandle for CanvasWindow {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        self.display_handle
    }
}

#[wasm_bindgen]
pub struct Renderer {
    instance: wgpu::Instance,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,

    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    ofield_pipeline: wgpu::RenderPipeline,

    buffers: Option<(wgpu::Buffer, wgpu::Buffer)>,
    num_indices: u32,

    ofield_buffers: Option<(wgpu::Buffer, wgpu::Buffer)>,
    num_ofield_indices: u32,

    mouse_down: bool,
    rx: f32,
    ry: f32,
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth16Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        label: None,
        view_formats: &[],
    });

    depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_view_transform(width: u32, height: u32) -> Mat4 {
    Mat4::perspective_rh(
        75f32.to_radians(),
        width as f32 / height as f32,
        0.1,
        1000.0,
    ) * Mat4::look_at_rh(vec3(0.0, 150.0, 0.0), Vec3::ZERO, Vec3::Z)
}

fn create_model_transform(rx: f32, ry: f32) -> Mat4 {
    Mat4::from_euler(glam::EulerRot::XYZ, ry, 0.0, rx)
}

#[wasm_bindgen]
impl Renderer {
    #[wasm_bindgen(constructor)]
    pub async fn new(canvas: &web_sys::HtmlCanvasElement) -> Renderer {
        let window = CanvasWindow::new(canvas);

        let instance = wgpu::Instance::default();
        let surface = unsafe { instance.create_surface(&window) }
            .expect("Failed to create WebGPU surface from canvas");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("No WebGPU adapter available");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                },
                None,
            )
            .await
            .expect("No suitable WebGPU device found!");

        let swap_caps = surface.get_capabilities(&adapter);
        let swap_format = swap_caps.formats[0];
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swap_format,
            width: canvas.width(),
            height: canvas.height(),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swap_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(128),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let depth_view = create_depth_texture(&device, surface_config.width, surface_config.height);

        let view_transform = create_view_transform(surface_config.width, surface_config.height);
        let model_transform = create_model_transform(0.0, 0.0);
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniforms"),
            contents: bytemuck::cast_slice(&[
                view_transform.to_cols_array(),
                model_transform.to_cols_array(),
            ]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("preview.wgsl"))),
        });

        let ofield_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("ofield.wgsl"))),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24 as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(swap_format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth16Unorm,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let ofield_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ofield_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 12 as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &ofield_shader,
                entry_point: "fs_main",
                targets: &[Some(swap_format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth16Unorm,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            surface_config,

            uniforms,
            bind_group,
            depth_view,
            pipeline,
            ofield_pipeline,

            buffers: None,
            num_indices: 0,

            ofield_buffers: None,
            num_ofield_indices: 0,

            mouse_down: false,
            rx: 0.0,
            ry: 0.0,
        }
    }

    /*
    pub fn run(mut self) -> ! {
        self.event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        self.surface_config.width = size.width;
                        self.surface_config.height = size.height;
                        self.surface.configure(&self.device, &self.surface_config);

                        let transform = create_transform(size.width, size.height, self.rx, self.ry);
                        self.queue.write_buffer(
                            &self.uniforms,
                            0,
                            bytemuck::cast_slice(&transform.to_cols_array()),
                        );

                        self.depth_view =
                            create_depth_texture(&self.device, size.width, size.height);

                        self.window.request_redraw();
                    }
                    WindowEvent::MouseInput {
                        state,
                        button: MouseButton::Left,
                        ..
                    } => self.mouse_down = state == ElementState::Pressed,
                    WindowEvent::CursorMoved { position, .. } => {
                        if self.mouse_down {
                            self.rx = (position.x / 500.0) as f32;
                            self.ry = (position.y / 500.0) as f32;

                            let transform = create_transform(
                                self.surface_config.width,
                                self.surface_config.height,
                                self.rx,
                                self.ry,
                            );
                            self.queue.write_buffer(
                                &self.uniforms,
                                0,
                                bytemuck::cast_slice(&transform.to_cols_array()),
                            );

                            self.window.request_redraw();
                        }
                    }
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    _ => (),
                },

                Event::RedrawRequested(_) => {
                    let frame = self
                        .surface
                        .get_current_texture()
                        .expect("Failed to acquire next swap chain texture");
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder = self
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: None,
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.1,
                                        g: 0.1,
                                        b: 0.1,
                                        a: 1.0,
                                    }),
                                    store: true,
                                },
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.depth_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: false,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                        });

                        if let Some((vertex_buf, index_buf)) = self.buffers.as_ref() {
                            rpass.set_pipeline(&self.pipeline);
                            rpass.set_bind_group(0, &self.bind_group, &[]);
                            rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
                            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                            rpass.draw_indexed(0..self.num_indices as u32, 0, 0..1);
                        }

                        if let Some((vertex_buf, index_buf)) = self.ofield_buffers.as_ref() {
                            rpass.set_pipeline(&self.ofield_pipeline);
                            rpass.set_bind_group(0, &self.bind_group, &[]);
                            rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
                            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                            rpass.draw_indexed(0..self.num_ofield_indices as u32, 0, 0..1);
                        }
                    }

                    self.queue.submit(Some(encoder.finish()));
                    frame.present();
                }

                Event::UserEvent(render_event) => match render_event {
                    RendererEvent::UploadMesh(mesh) => {
                        // Assemble data in a more GPU-friendly manner
                        assert!(mesh.vertices.len() - 1 <= u32::MAX as usize);
                        let verts = mesh
                            .vertices
                            .iter()
                            .zip(mesh.normals)
                            .map(|(v, n)| [v.to_array(), n.to_array()])
                            .collect::<Vec<_>>();
                        let indices = mesh
                            .tris
                            .iter()
                            .map(|[a, b, c]| [*a as u32, *b as u32, *c as u32]) // necessary?
                            .collect::<Vec<_>>();

                        self.buffers = Some((
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Mesh vertices"),
                                    contents: bytemuck::cast_slice(verts.as_slice()),
                                    usage: wgpu::BufferUsages::VERTEX,
                                }),
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Mesh indices"),
                                    contents: bytemuck::cast_slice(indices.as_slice()),
                                    usage: wgpu::BufferUsages::INDEX,
                                }),
                        ));
                        self.num_indices = (mesh.tris.len() * 3) as u32;

                        self.window.request_redraw();
                    }

                    RendererEvent::UploadOField(p, n, o) => {
                        let mut vertices = Vec::new();
                        let mut indices = Vec::new();

                        let mut rng = SmallRng::seed_from_u64(0);
                        for (i, p) in p.iter().enumerate() {
                            if rng.gen::<f32>() > 0.95 {
                                let n = n[i];
                                let o = o[i];

                                let v = n.cross(o);

                                vertices.push((*p + 3.0 * o - 0.1 * v).to_array());
                                vertices.push((*p - 3.0 * o - 0.1 * v).to_array());
                                vertices.push((*p + 3.0 * o + 0.1 * v).to_array());
                                vertices.push((*p - 3.0 * o + 0.1 * v).to_array());
                                vertices.push((*p + 3.0 * v - 0.1 * o).to_array());
                                vertices.push((*p - 3.0 * v - 0.1 * o).to_array());
                                vertices.push((*p + 3.0 * v + 0.1 * o).to_array());
                                vertices.push((*p - 3.0 * v + 0.1 * o).to_array());

                                let l = vertices.len() as u32;
                                indices.push(l - 8);
                                indices.push(l - 7);
                                indices.push(l - 6);
                                indices.push(l - 6);
                                indices.push(l - 5);
                                indices.push(l - 7);
                                indices.push(l - 4);
                                indices.push(l - 3);
                                indices.push(l - 2);
                                indices.push(l - 2);
                                indices.push(l - 1);
                                indices.push(l - 3);
                            }
                        }

                        // Assemble data in a more GPU-friendly manner
                        self.ofield_buffers = Some((
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Ofield vertices"),
                                    contents: bytemuck::cast_slice(vertices.as_slice()),
                                    usage: wgpu::BufferUsages::VERTEX,
                                }),
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Ofield indices"),
                                    contents: bytemuck::cast_slice(indices.as_slice()),
                                    usage: wgpu::BufferUsages::INDEX,
                                }),
                        ));
                        self.num_ofield_indices = indices.len() as u32;

                        self.window.request_redraw();
                    }
                },

                _ => {}
            }
        });
    }
    */

    #[wasm_bindgen]
    pub fn draw(&self) {
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: false,
                    }),
                    stencil_ops: None,
                }),
            });

            if let Some((vertex_buf, index_buf)) = self.buffers.as_ref() {
                rpass.set_pipeline(&self.pipeline);
                rpass.set_bind_group(0, &self.bind_group, &[]);
                rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                rpass.draw_indexed(0..self.num_indices as u32, 0, 0..1);
            }

            if let Some((vertex_buf, index_buf)) = self.ofield_buffers.as_ref() {
                rpass.set_pipeline(&self.ofield_pipeline);
                rpass.set_bind_group(0, &self.bind_group, &[]);
                rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
                rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                rpass.draw_indexed(0..self.num_ofield_indices as u32, 0, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    #[wasm_bindgen]
    pub fn update_mesh(&mut self, mesh: &InputMesh) {
        // Assemble data in a more GPU-friendly manner
        assert!(mesh.vertices.len() - 1 <= u32::MAX as usize);
        let verts = mesh
            .vertices
            .iter()
            .zip(&mesh.normals)
            .map(|(v, n)| [v.to_array(), n.to_array()])
            .collect::<Vec<_>>();
        let indices = mesh
            .tris
            .iter()
            .map(|[a, b, c]| [*a as u32, *b as u32, *c as u32]) // necessary?
            .collect::<Vec<_>>();

        self.buffers = Some((
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh vertices"),
                    contents: bytemuck::cast_slice(verts.as_slice()),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh indices"),
                    contents: bytemuck::cast_slice(indices.as_slice()),
                    usage: wgpu::BufferUsages::INDEX,
                }),
        ));
        self.num_indices = (mesh.tris.len() * 3) as u32;
    }

    #[wasm_bindgen]
    pub fn orbit_camera(&mut self, dx: f32, dy: f32) {
        self.rx += dx / 200.0;
        self.ry -= dy / 200.0;

        let view_transform =
            create_view_transform(self.surface_config.width, self.surface_config.height);
        let model_transform = create_model_transform(self.rx, self.ry);
        self.queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::cast_slice(&[
                view_transform.to_cols_array(),
                model_transform.to_cols_array(),
            ]),
        );
    }
}
