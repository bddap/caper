use glium::{Display, DrawParameters, Surface, Depth, Blend};
use glium::index::{NoIndices, PrimitiveType};
use glium::DepthTest::IfLess;
use glium::vertex::VertexBuffer;
use glium::glutin::{WindowBuilder, ContextBuilder, EventsLoop, GlRequest, Api};
use glium::glutin::CursorState::Hide;
use glium::draw_parameters::{DepthClamp, BackfaceCullingMode};
use glium::texture::RawImage2d;


use glium_text;
use glium_text::{TextSystem, FontTexture, TextDisplay};

use time;
use std::default::Default;
use fps_counter::FPSCounter;

use imgui::*;
use imgui_glium_renderer::Renderer as ImGuiRenderer;

use image;
use gif;
use gif::SetParameter;

use std::path::Path;
use std::fs::File;
use std::fs::OpenOptions;
use std::thread;

use shader::Shaders;
use utils::{build_persp_proj_mat, build_fp_view_matrix, mul_mat4, frustrum_test,
            get_frustum_planes};
use posteffect::{PostEffect, render_to_texture};
use types::{RenderItem, TextItem, Camera, ShaderIn, PhysicsType};
use lighting::Lighting;
use input::{Input, MouseButton};


/// struct for abstracting the render state
pub struct Renderer {
    /// The glium display used for rendering
    pub display: Display,
    /// The glium_text system used for rendering TextItem
    pub text_system: TextSystem,
    /// Fefault font that the text renderer will use
    default_font: FontTexture,
    /// Main imgui system
    imgui: ImGui,
    /// The sub renderer for imgui
    imgui_rend: ImGuiRenderer,
    /// Instance of PostEffect used for rendering post processing
    pub post_effect: PostEffect,
    /// The render/engine start time
    pub start_time: f64,
    /// The shaders that can be used for rendering
    pub shaders: Shaders,
    /// stuct to track the fps
    fps_counter: FPSCounter,
    /// The current frames per second the Renderer is drawing at
    pub fps: f32,
    /// Info on the current gif being written to
    gif_info: Option<GifInfo>,
    /// The lighting system
    pub lighting: Lighting,
    /// The number items rendered in the last drawn frame
    pub render_count: usize,
    /// Whether to display the engine editor window
    pub show_editor: bool,
}

struct GifInfo {
    /// The encoder for the current gif
    encoder: gif::Encoder<File>,
    /// The path of the current gif
    path: &'static str,
}

impl Renderer {
    /// Creates new Renderer instance
    pub fn new(title: String) -> (Renderer, EventsLoop) {
        let events_loop = EventsLoop::new();
        let window_builder = WindowBuilder::new().with_title(title).with_fullscreen(
            Some(
                events_loop.get_primary_monitor(),
            ),
        );
        let context = ContextBuilder::new()
            .with_depth_buffer(24)
            .with_vsync(true)
            .with_gl(GlRequest::Specific(Api::OpenGl, (4, 0)));
        let display = Display::new(window_builder, context, &events_loop).unwrap();

        // create a text system instance and font
        let text_system = TextSystem::new(&display);
        let font = FontTexture::new(
            &display,
            &include_bytes!("./resources/font.ttf")[..],
            100,
            glium_text::FontTexture::ascii_character_list(),
        ).unwrap();

        let mut imgui = ImGui::init();
        let imgui_rend = ImGuiRenderer::init(&mut imgui, &display).unwrap();

        let shaders = Shaders::new(&display);
        let post_fx = PostEffect::new(&display);
        let lighting = Lighting::new(&display);

        let fps_counter = FPSCounter::new();

        let renderer = Renderer {
            display: display,
            text_system: text_system,
            default_font: font,
            imgui: imgui,
            imgui_rend: imgui_rend,
            post_effect: post_fx,
            start_time: time::precise_time_s(),
            shaders: shaders,
            fps_counter: fps_counter,
            fps: 0f32,
            gif_info: None,
            lighting: lighting,
            render_count: 0usize,
            show_editor: false,
        };

        {
            let window = renderer.display.gl_window();
            window.set_cursor_state(Hide).ok();
        }

        (renderer, events_loop)
    }

    /// Update imgui's interal input state
    pub fn update_imgui_input(&mut self, input: &Input) {
        self.imgui.set_mouse_pos(
            input.mouse_pos.0,
            input.mouse_pos.1,
        );
        self.imgui.set_mouse_down(
            &[
                input.mouse_btns_down.contains(&MouseButton::Left),
                input.mouse_btns_down.contains(&MouseButton::Right),
                input.mouse_btns_down.contains(&MouseButton::Middle),
                false,
                false,
            ],
        );
        for i in 0..input.characters_down.len() {
            self.imgui.add_input_character(input.characters_down[i])
        }
    }

    /// Draws a frame
    pub fn draw<F: FnMut(&Ui)>(
        &mut self,
        cams: &mut Vec<Camera>,
        render_items: &mut Vec<RenderItem>,
        text_items: &mut Vec<TextItem>,
        mut f: F,
    ) {
        // get display dimensions
        let (width, height) = self.display.get_framebuffer_dimensions();

        // draw parameters
        let params = DrawParameters {
            depth: Depth {
                test: IfLess,
                write: true,
                clamp: DepthClamp::Clamp,
                ..Default::default()
            },
            blend: Blend::alpha_blending(),
            backface_culling: BackfaceCullingMode::CullClockwise,
            ..Default::default()
        };

        // drawing a frame
        let mut target = self.display.draw();
        let mut render_count = 0usize;
        let mut cols = Vec::new();
        let mut depths = Vec::new();

        for c in 0..cams.len() {
            let cam = cams[c];
            // uniforms passed to the shaders
            let projection_matrix =
                build_persp_proj_mat(60f32, width as f32 / height as f32, 0.01f32, 1000f32);
            let modelview_matrix = build_fp_view_matrix(&cam);
            let cam_pos = cam.pos;
            let time = (time::precise_time_s() - self.start_time) as f32;

            // calc frustum places for culling
            let combo_matrix = mul_mat4(projection_matrix, modelview_matrix);
            let frustum_planes = get_frustum_planes(&combo_matrix);

            // render to texture/depth
            let (target_color, target_depth) =
                render_to_texture(&self.post_effect, &mut target, |target| {
                    // clear the colour and depth buffers
                    target.clear_color_and_depth((1.0, 1.0, 1.0, 1.0), 1.0);

                    // drawing the render items (with more than one instance)
                    for item in render_items.iter().filter(|r| {
                        r.active && r.instance_transforms.len() > 0
                    })
                    {
                        // building the vertex and index buffers
                        let vertex_buffer = VertexBuffer::new(&self.display, &item.vertices)
                            .unwrap();

                        // add positions for instances
                        let per_instance = {
                            let data = item.instance_transforms
                                .iter()
                                .filter(|t| {
                                    (t.active && !t.cull) ||
                                        (t.active &&
                                             frustrum_test(
                                                &t.pos,
                                                t.scale.0.max(t.scale.1.max(t.scale.2)) * 2.5f32,
                                                &frustum_planes,
                                            ))
                                })
                                .map(|t| {
                                    ShaderIn {
                                        world_position: t.pos,
                                        world_rotation: t.rot,
                                        world_scale: t.scale,
                                    }
                                })
                                .collect::<Vec<_>>();

                            // if there are no active transforms skip ri
                            if data.len() <= 0 {
                                continue;
                            }

                            // add instances to render_count
                            render_count += data.len();

                            VertexBuffer::dynamic(&self.display, &data).unwrap()
                        };

                        let tex_name = item.material.texture_name.clone().unwrap_or(
                            "default".to_string(),
                        );
                        let normal_tex_name = item.material.normal_texture_name.clone().unwrap_or(
                            "default_normal".to_string(),
                        );

                        let dir_lights = self.lighting.directional_tex.borrow();

                        let uniforms =
                            uniform! {
                                    projection_matrix: projection_matrix,
                                    modelview_matrix: modelview_matrix,
                                    cam_pos: cam_pos,
                                    viewport: (width as f32, height as f32),
                                    time: time,
                                    tex: self.shaders.textures.get(tex_name.as_str()).unwrap(),
                                    normal_tex:
                                        self.shaders.textures.get(normal_tex_name.as_str())
                                        .unwrap(),
                                    dir_lights: &*dir_lights,
                                };

                        target
                            .draw(
                                (&vertex_buffer, per_instance.per_instance().unwrap()),
                                &NoIndices(PrimitiveType::Patches { vertices_per_patch: 3 }),
                                &self.shaders
                                    .shaders
                                    .get(item.material.shader_name.as_str())
                                    .unwrap(),
                                &uniforms,
                                &params,
                            )
                            .unwrap();
                    }
                });

            cols.push(target_color);
            depths.push(target_depth);
        }

        //let texs_arr = Texture2dArray::new(&self.post_effect.context, cols).unwrap();
        //let depths_arr = DepthTexture2dArray::new(&self.post_effect.context, depths).unwrap();

        // second pass draw the post effect and composition
        let uniforms =
            uniform! {
                // general uniforms
                tex: &cols[0],
                depth_buf: &depths[0],
                resolution: (width as f32, height as f32),
                time: time::precise_time_s() as f32 - self.post_effect.start_time,
                downscale_factor: self.post_effect.downscale_factor,
                // post effect param uniforms
                chrom_offset: self.post_effect.post_shader_options.chrom_offset,
                chrom_amt: self.post_effect.post_shader_options.chrom_amt,
                blur: self.post_effect.post_shader_options.blur,
                blur_amt: self.post_effect.post_shader_options.blur_amt,
                blur_radius: self.post_effect.post_shader_options.blur_radius,
                blur_weight: self.post_effect.post_shader_options.blur_weight,
                bokeh: self.post_effect.post_shader_options.bokeh,
                bokeh_focal_depth: self.post_effect.post_shader_options.bokeh_focal_depth,
                bokeh_focal_width: self.post_effect.post_shader_options.bokeh_focal_width,
                color_offset: self.post_effect.post_shader_options.color_offset,
                greyscale: self.post_effect.post_shader_options.greyscale,
            };

        let uniforms = if cols.len() > 1 {
            uniforms.add("tex_1", &cols[1]).add(
                "depth_buf_1",
                &depths[1],
            )
        } else {
            uniforms.add("tex_1", &cols[0]).add(
                "depth_buf_1",
                &depths[0],
            )
        };

        let uniforms = if cols.len() > 2 {
            uniforms.add("tex_2", &cols[2]).add(
                "depth_buf_2",
                &depths[2],
            )
        } else {
            uniforms.add("tex_2", &cols[0]).add(
                "depth_buf_2",
                &depths[0],
            )
        };

        let uniforms = if cols.len() > 3 {
            uniforms.add("tex_3", &cols[3]).add(
                "depth_buf_3",
                &depths[3],
            )
        } else {
            uniforms.add("tex_3", &cols[0]).add(
                "depth_buf_3",
                &depths[0],
            )
        };

        let uniforms = if cols.len() > 4 {
            uniforms.add("tex_4", &cols[4]).add(
                "depth_buf_4",
                &depths[4],
            )
        } else {
            uniforms.add("tex_4", &cols[0]).add(
                "depth_buf_4",
                &depths[0],
            )
        };

        let uniforms = if cols.len() > 5 {
            uniforms.add("tex_5", &cols[5]).add(
                "depth_buf_5",
                &depths[5],
            )
        } else {
            uniforms.add("tex_5", &cols[0]).add(
                "depth_buf_5",
                &depths[0],
            )
        };

        target
            .draw(
                &self.post_effect.vertex_buffer,
                &self.post_effect.index_buffer,
                &self.shaders
                    .post_shaders
                    .get(self.post_effect.current_shader)
                    .unwrap(),
                &uniforms,
                &Default::default(),
            )
            .unwrap();

        self.render_count = render_count;

        // drawing the text items
        for text_item in text_items.iter().filter(|r| r.active) {
            // create the matrix for the text
            let matrix = [
                [0.02 * text_item.scale.0, 0.0, 0.0, 0.0],
                [
                    0.0,
                    0.02 * text_item.scale.1 * (width as f32) / (height as f32),
                    0.0,
                    0.0,
                ],
                [0.0, 0.0, 0.02 * text_item.scale.2, 0.0],
                [text_item.pos.0, text_item.pos.1, text_item.pos.2, 1.0f32],
            ];

            // create TextDisplay for item, TODO change this to not be done every frame
            let text = TextDisplay::new(
                &self.text_system,
                &self.default_font,
                text_item.text.as_str(),
            );

            // draw the text
            let _ = glium_text::draw(
                &text,
                &self.text_system,
                &mut target,
                matrix,
                text_item.color,
            );
        }

        // imgui elements
        let ui = self.imgui.frame((width, height), (width, height), 0.1);
        f(&ui);

        // create the engine editor
        if self.show_editor {
            // create the editor window
            ui.window(im_str!("caper editor"))
                .size((300.0, 200.0), ImGuiSetCond_FirstUseEver)
                .position((0.0, 0.0), ImGuiSetCond_FirstUseEver)
                .build(|| {
                    // camera state editor
                    if ui.collapsing_header(im_str!("Camera")).build() {
                        for cam in cams {
                            if ui.collapsing_header(im_str!("Cam")).build() {
                                // camera position
                                if ui.collapsing_header(im_str!("position")).build() {
                                    ui.input_float(im_str!("x"), &mut cam.pos.0)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("y"), &mut cam.pos.1)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("z"), &mut cam.pos.2)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                }
                                // camera rotation
                                if ui.collapsing_header(im_str!("rotation")).build() {
                                    ui.input_float(im_str!("x"), &mut cam.euler_rot.0)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("y"), &mut cam.euler_rot.1)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("z"), &mut cam.euler_rot.2)
                                        .step(0.1)
                                        .step_fast(1.0)
                                        .build();
                                }
                            }
                        }
                    }
                    // render items editor
                    if ui.collapsing_header(im_str!("Render items")).build() {
                        // create node for each item
                        for render_item in render_items {
                            ui.tree_node(im_str!("name:{}", render_item.name)).build(
                                || {
                                    ui.checkbox(im_str!("active"), &mut render_item.active);
                                    // physics type TODO make sure this is propagated
                                    let mut physics_type = match render_item.physics_type {
                                        PhysicsType::Static => 0,
                                        PhysicsType::Dynamic => 1,
                                        PhysicsType::None => 2,
                                    };
                                    ui.combo(
                                        im_str!("physics"),
                                        &mut physics_type,
                                        &[im_str!("Static"), im_str!("Dynamic"), im_str!("None")],
                                        -1,
                                    );
                                    render_item.physics_type = match physics_type {
                                        0 => PhysicsType::Static,
                                        1 => PhysicsType::Dynamic,
                                        _ => PhysicsType::None,
                                    };
                                    // TODO add mutability for these items
                                    ui.text(im_str!(
                                        "instance_count:{}",
                                        render_item.instance_transforms.len()
                                    ));
                                    ui.text(im_str!("vert_count:{}", render_item.vertices.len()));
                                },
                            );
                        }
                    }
                    // text items editor
                    if ui.collapsing_header(im_str!("Text items")).build() {
                        for text_item in text_items {
                            ui.tree_node(im_str!("name:{}", text_item.name)).build(|| {
                                // TODO add mutability
                                //ui.input_text(im_str!("text"), &mut text_item.text).build();
                                // text item color
                                if ui.collapsing_header(im_str!("color")).build() {
                                    ui.input_float(im_str!("r"), &mut text_item.color.0)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("g"), &mut text_item.color.1)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("b"), &mut text_item.color.2)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("a"), &mut text_item.color.3)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                }
                                // text item position
                                if ui.collapsing_header(im_str!("position")).build() {
                                    ui.input_float(im_str!("x"), &mut text_item.pos.0)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("y"), &mut text_item.pos.1)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("z"), &mut text_item.pos.2)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                }
                                // text item scale
                                if ui.collapsing_header(im_str!("scale")).build() {
                                    ui.input_float(im_str!("x"), &mut text_item.scale.0)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("y"), &mut text_item.scale.1)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                    ui.input_float(im_str!("z"), &mut text_item.scale.2)
                                        .step(0.01)
                                        .step_fast(1.0)
                                        .build();
                                }
                                ui.checkbox(im_str!("active"), &mut text_item.active);
                            });
                        }
                    }
                });
        }

        // render imgui items
        self.imgui_rend.render(&mut target, ui).unwrap();

        match target.finish() {
            Ok(_) => {
                self.fps = self.fps_counter.tick() as f32;
            }
            Err(e) => println!("{:?}", e),
        };
    }

    /// Saves out a screenshot from in-game
    pub fn save_screenshot(&self) {
        // reading the front buffer into an image
        let image: RawImage2d<u8> = self.display.read_front_buffer();

        thread::spawn(move || {
            let image =
                image::ImageBuffer::from_raw(image.width, image.height, image.data.into_owned())
                    .unwrap();
            let image = image::DynamicImage::ImageRgba8(image).flipv();
            let mut output = File::create(&Path::new(
                format!("./screenshot_{}.png", time::precise_time_s())
                    .as_str(),
            )).unwrap();
            image.save(&mut output, image::ImageFormat::PNG).unwrap();
        });
    }

    /// When called with the same path adds a frame to a gif at the path
    pub fn save_add_to_gif(&mut self, path: &'static str) {
        // reading the front buffer into a gif frame
        let image: RawImage2d<u8> = self.display.read_front_buffer();

        let (w, h) = (image.width, image.height);

        let mut image = {
            let image_buf = image::ImageBuffer::from_raw(w, h, image.data.into_owned()).unwrap();
            let dy_image = image::DynamicImage::ImageRgba8(image_buf).flipv();
            let fin_image = dy_image.as_rgba8().unwrap();
            fin_image.clone().into_raw()
        };
        let frame = gif::Frame::from_rgba(w as u16, h as u16, image.as_mut_slice());

        // if there is no encoder present create one
        let new_file = {
            match self.gif_info.as_ref() {
                Some(gi_ref) => gi_ref.path != path,
                None => false,
            }
        };
        if self.gif_info.is_none() || new_file {
            let output = OpenOptions::new()
                .write(true)
                .create(true)
                .open(path)
                .unwrap();
            let mut encoder = gif::Encoder::new(output, w as u16, h as u16, &[]).unwrap();
            encoder.set(gif::Repeat::Infinite).unwrap();

            let info = GifInfo {
                encoder: encoder,
                path: path,
            };

            self.gif_info = Some(info);
        }
        // Write frame to file
        if let Some(ref mut info) = self.gif_info {
            info.encoder.write_frame(&frame).unwrap();
        }
    }
}
