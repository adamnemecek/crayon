use std::sync::Arc;

use crayon::{graphics, application, utils};

use imgui::{DrawList, ImGui, Ui};
use errors::*;

impl_vertex!{
    CanvasVertex {
        position => [Position; Float; 2; false],
        texcoord => [Texcoord0; Float; 2; false],
        color => [Color0; UByte; 4; true],
    }
}

pub struct Renderer {
    video: Arc<graphics::GraphicsSystemShared>,

    surface: graphics::SurfaceHandle,
    shader: graphics::ShaderHandle,
    texture: graphics::TextureHandle,

    mesh: Option<(u32, u32, graphics::MeshHandle)>,
}

impl Renderer {
    /// Creates a new `CanvasRenderer`. This will allocates essential video
    /// resources in background.
    pub fn new(ctx: &application::Context, imgui: &mut ImGui) -> Result<Self> {
        let video = ctx.shared::<graphics::GraphicsSystem>();

        let mut setup = graphics::SurfaceSetup::default();
        setup.set_clear(utils::Color::white(), None, None);
        setup.set_sequence(true);
        let surface = video.create_surface(setup)?;

        let layout = graphics::AttributeLayoutBuilder::new()
            .with(graphics::VertexAttribute::Position, 2)
            .with(graphics::VertexAttribute::Texcoord0, 2)
            .with(graphics::VertexAttribute::Color0, 4)
            .finish();

        let mut setup = graphics::ShaderSetup::default();
        setup.layout = layout;
        setup.render_state.cull_face = graphics::CullFace::Back;
        setup.render_state.front_face_order = graphics::FrontFaceOrder::Clockwise;
        setup.render_state.color_blend =
            Some((graphics::Equation::Add,
                  graphics::BlendFactor::Value(graphics::BlendValue::SourceAlpha),
                  graphics::BlendFactor::OneMinusValue(graphics::BlendValue::SourceAlpha)));

        setup.vs = include_str!("../resources/imgui.vs").to_owned();
        setup.fs = include_str!("../resources/imgui.fs").to_owned();
        setup.uniform_variables.push("matrix".into());
        setup.uniform_variables.push("texture".into());
        let shader = video.create_shader(setup)?;

        let texture = imgui
            .prepare_texture(|v| {
                                 let mut setup = graphics::TextureSetup::default();
                                 setup.dimensions = (v.width, v.height);
                                 setup.filter = graphics::TextureFilter::Nearest;
                                 setup.format = graphics::TextureFormat::U8U8U8U8;
                                 video.create_texture(setup, Some(v.pixels))
                             })?;

        imgui.set_texture_id(**texture as usize);

        Ok(Renderer {
               video: video.clone(),

               surface: surface,
               shader: shader,
               texture: texture,
               mesh: None,
           })
    }

    pub fn render<'a>(&mut self, ui: Ui<'a>) -> Result<()> {
        ui.render(|ui, dcs| self.render_draw_list(ui, &dcs))
    }

    fn render_draw_list<'a>(&mut self, ui: &'a Ui<'a>, tasks: &DrawList<'a>) -> Result<()> {
        let mut verts = Vec::with_capacity(tasks.vtx_buffer.len());

        for v in tasks.vtx_buffer {
            let color = utils::Color::from_abgr_u32(v.col).into();
            verts.push(CanvasVertex::new([v.pos.x, v.pos.y], [v.uv.x, v.uv.y], color));
        }

        let mesh = self.update_mesh(&verts, &tasks.idx_buffer)?;
        let (width, height) = ui.imgui().display_size();
        let (scale_width, scale_height) = ui.imgui().display_framebuffer_scale();

        if width == 0.0 || height == 0.0 {
            return Ok(());
        }

        let matrix = graphics::UniformVariable::Matrix4f([[2.0 / width as f32, 0.0, 0.0, 0.0],
                                                          [0.0, 2.0 / -(height as f32), 0.0, 0.0],
                                                          [0.0, 0.0, -1.0, 0.0],
                                                          [-1.0, 1.0, 0.0, 1.0]],
                                                         false);

        let font_texture_id = **self.texture as usize;
        let mut idx_start = 0;
        for cmd in tasks.cmd_buffer {
            assert!(font_texture_id == cmd.texture_id as usize);

            let scissor_pos = ((cmd.clip_rect.x * scale_width) as u16,
                               ((height - cmd.clip_rect.w) * scale_height) as u16);
            let scissor_size = (((cmd.clip_rect.z - cmd.clip_rect.x) * scale_width) as u16,
                                ((cmd.clip_rect.w - cmd.clip_rect.y) * scale_height) as u16);

            {
                let scissor = graphics::Scissor::Enable(scissor_pos, scissor_size);
                let cmd = graphics::Command::set_scissor(scissor);
                self.video.submit(self.surface, 0, cmd)?;
            }

            {
                let mut dc = graphics::DrawCall::new(self.shader, mesh);
                dc.set_uniform_variable("matrix", matrix);
                dc.set_uniform_variable("texture", self.texture);
                let cmd = dc.build(idx_start, cmd.elem_count)?;
                self.video.submit(self.surface, 0, cmd)?;
            }

            idx_start += cmd.elem_count;
        }

        Ok(())
    }

    fn update_mesh(&mut self,
                   verts: &[CanvasVertex],
                   idxes: &[u16])
                   -> Result<graphics::MeshHandle> {
        if let Some((nv, ni, handle)) = self.mesh {
            if nv >= verts.len() as u32 && ni >= idxes.len() as u32 {
                let slice = CanvasVertex::as_bytes(verts);
                let cmd = graphics::Command::update_vertex_buffer(handle, 0, slice);
                self.video.submit(self.surface, 0, cmd)?;

                let slice = graphics::IndexFormat::as_bytes(idxes);
                let cmd = graphics::Command::update_index_buffer(handle, 0, slice);
                self.video.submit(self.surface, 0, cmd)?;

                return Ok(handle);
            }

            self.video.delete_mesh(handle);
        }

        let mut nv = 1;
        while nv < verts.len() as u32 {
            nv *= 2;
        }

        let mut ni = 1;
        while ni < idxes.len() as u32 {
            ni *= 2;
        }

        let mut setup = graphics::MeshSetup::default();
        setup.hint = graphics::BufferHint::Stream;
        setup.layout = CanvasVertex::layout();
        setup.index_format = graphics::IndexFormat::U16;
        setup.primitive = graphics::Primitive::Triangles;
        setup.num_vertices = nv;
        setup.num_indices = ni;

        let verts_slice = CanvasVertex::as_bytes(verts);
        let idxes_slice = graphics::IndexFormat::as_bytes(idxes);
        let mesh = self.video.create_mesh(setup, verts_slice, idxes_slice)?;
        self.mesh = Some((nv, ni, mesh));
        Ok(mesh)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.video.delete_surface(self.surface);
        self.video.delete_shader(self.shader);
        self.video.delete_texture(self.texture);

        if let Some((_, _, mesh)) = self.mesh {
            self.video.delete_mesh(mesh);
        }
    }
}