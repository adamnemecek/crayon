use std::sync::{Arc, RwLock};
use uuid::Uuid;

use application::prelude::{LifecycleListener, LifecycleListenerHandle};
use math::prelude::{Aabb2, Vector2};
use res::utils::Registry;
use utils::prelude::{DoubleBuf, ObjectPool};

use super::assets::mesh_loader::MeshLoader;
use super::assets::prelude::*;
use super::assets::texture_loader::TextureLoader;
use super::backends::frame::*;
use super::backends::{self, Visitor};
use super::errors::*;

/// The centralized management of video sub-system.
pub struct VideoSystem {
    lis: LifecycleListenerHandle,
    state: Arc<VideoState>,
}

type TextureRegistry = Registry<TextureHandle, TextureLoader>;
type MeshRegistry = Registry<MeshHandle, MeshLoader>;

struct VideoState {
    frames: Arc<DoubleBuf<Frame>>,
    surfaces: RwLock<ObjectPool<SurfaceHandle, SurfaceParams>>,
    shaders: RwLock<ObjectPool<ShaderHandle, ShaderParams>>,
    meshes: MeshRegistry,
    textures: TextureRegistry,
    render_textures: RwLock<ObjectPool<RenderTextureHandle, RenderTextureParams>>,
}

impl VideoState {
    fn new() -> Self {
        let frames = Arc::new(DoubleBuf::new(
            Frame::with_capacity(64 * 1024),
            Frame::with_capacity(64 * 1024),
        ));

        let textures = TextureRegistry::new(TextureLoader::new(frames.clone()));
        let meshes = MeshRegistry::new(MeshLoader::new(frames.clone()));

        VideoState {
            frames: frames,
            surfaces: RwLock::new(ObjectPool::new()),
            shaders: RwLock::new(ObjectPool::new()),
            meshes: meshes,
            textures: textures,
            render_textures: RwLock::new(ObjectPool::new()),
        }
    }
}

struct Lifecycle {
    last_dimensions: Vector2<u32>,
    visitor: Box<dyn Visitor>,
    state: Arc<VideoState>,
}

impl LifecycleListener for Lifecycle {
    fn on_pre_update(&mut self) -> crate::errors::Result<()> {
        // Swap internal commands frame.
        self.state.frames.swap();
        self.state.frames.write().clear();
        Ok(())
    }

    fn on_post_update(&mut self) -> crate::errors::Result<()> {
        let dimensions = dimensions_pixels();

        // Resize the window, which would recreate the underlying framebuffer.
        if dimensions != self.last_dimensions {
            self.last_dimensions = dimensions;
            crate::window::resize(dimensions);
        }

        self.state
            .frames
            .write_back_buf()
            .dispatch(self.visitor.as_mut(), self.last_dimensions)?;

        Ok(())
    }
}

impl Drop for VideoSystem {
    fn drop(&mut self) {
        crate::application::detach(self.lis);
    }
}

impl VideoSystem {
    /// Create a new `VideoSystem`.
    pub fn new() -> ::errors::Result<Self> {
        let state = Arc::new(VideoState::new());
        let visitor = backends::new()?;

        Ok(VideoSystem {
            state: state.clone(),
            lis: crate::application::attach(Lifecycle {
                last_dimensions: dimensions_pixels(),
                state: state,
                visitor: visitor,
            }),
        })
    }

    /// Create a headless `VideoSystem`.
    pub fn headless() -> Self {
        let state = Arc::new(VideoState::new());
        let visitor = backends::new_headless();

        VideoSystem {
            state: state.clone(),
            lis: crate::application::attach(Lifecycle {
                last_dimensions: Vector2::new(0, 0),
                state: state,
                visitor: visitor,
            }),
        }
    }

    pub(crate) fn frames(&self) -> Arc<DoubleBuf<Frame>> {
        self.state.frames.clone()
    }
}

impl VideoSystem {
    /// Creates an surface with `SurfaceParams`.
    pub fn create_surface(&self, params: SurfaceParams) -> Result<SurfaceHandle> {
        let handle = self.state.surfaces.write().unwrap().create(params).into();

        {
            let cmd = Command::CreateSurface(handle, params);
            self.state.frames.write().cmds.push(cmd);
        }

        Ok(handle)
    }

    /// Gets the `SurfaceParams` if available.
    pub fn surface(&self, handle: SurfaceHandle) -> Option<SurfaceParams> {
        self.state.surfaces.read().unwrap().get(handle).cloned()
    }

    /// Deletes surface object.
    pub fn delete_surface(&self, handle: SurfaceHandle) {
        if self.state.surfaces.write().unwrap().free(handle).is_some() {
            let cmd = Command::DeleteSurface(handle);
            self.state.frames.write().cmds.push(cmd);
        }
    }
}

impl VideoSystem {
    /// Create a shader with initial shaders and render state. It encapusulates all the
    /// informations we need to configurate graphics pipeline before real drawing.
    pub fn create_shader(
        &self,
        params: ShaderParams,
        vs: String,
        fs: String,
    ) -> Result<ShaderHandle> {
        params.validate(&vs, &fs)?;

        let handle = self.state.shaders.write().unwrap().create(params.clone());

        {
            let cmd = Command::CreateShader(handle, params, vs, fs);
            self.state.frames.write().cmds.push(cmd);
        }

        Ok(handle)
    }

    /// Gets the `ShaderParams` if available.
    pub fn shader(&self, handle: ShaderHandle) -> Option<ShaderParams> {
        self.state.shaders.read().unwrap().get(handle).cloned()
    }

    /// Delete shader state object.
    pub fn delete_shader(&self, handle: ShaderHandle) {
        if self.state.shaders.write().unwrap().free(handle).is_some() {
            let cmd = Command::DeleteShader(handle);
            self.state.frames.write().cmds.push(cmd);
        }
    }
}

impl VideoSystem {
    /// Create a new mesh object.
    #[inline]
    pub fn create_mesh<T>(&self, params: MeshParams, data: T) -> ::errors::Result<MeshHandle>
    where
        T: Into<Option<MeshData>>,
    {
        let handle = self.state.meshes.create((params, data.into()))?;
        Ok(handle)
    }

    /// Creates a mesh object from file asynchronously.
    #[inline]
    pub fn create_mesh_from<T: AsRef<str>>(&self, url: T) -> ::errors::Result<MeshHandle> {
        let handle = self.state.meshes.create_from(url)?;
        Ok(handle)
    }

    /// Creates a mesh object from file asynchronously.
    #[inline]
    pub fn create_mesh_from_uuid(&self, uuid: Uuid) -> ::errors::Result<MeshHandle> {
        let handle = self.state.meshes.create_from_uuid(uuid)?;
        Ok(handle)
    }

    /// Gets the `MeshParams` if available.
    #[inline]
    pub fn mesh(&self, handle: MeshHandle) -> Option<MeshParams> {
        self.state.meshes.get(handle, |v| v.clone())
    }

    /// Update a subset of dynamic vertex buffer. Use `offset` specifies the offset
    /// into the buffer object's data store where data replacement will begin, measured
    /// in bytes.
    pub fn update_vertex_buffer(
        &self,
        handle: MeshHandle,
        offset: usize,
        data: &[u8],
    ) -> ::errors::Result<()> {
        self.state
            .meshes
            .get(handle, |_| {
                let mut frame = self.state.frames.write();
                let ptr = frame.bufs.extend_from_slice(data);
                let cmd = Command::UpdateVertexBuffer(handle, offset, ptr);
                frame.cmds.push(cmd);
            }).ok_or_else(|| format_err!("{:?}", handle))
    }

    /// Update a subset of dynamic index buffer. Use `offset` specifies the offset
    /// into the buffer object's data store where data replacement will begin, measured
    /// in bytes.
    pub fn update_index_buffer(
        &self,
        handle: MeshHandle,
        offset: usize,
        data: &[u8],
    ) -> ::errors::Result<()> {
        self.state
            .meshes
            .get(handle, |_| {
                let mut frame = self.state.frames.write();
                let ptr = frame.bufs.extend_from_slice(data);
                let cmd = Command::UpdateIndexBuffer(handle, offset, ptr);
                frame.cmds.push(cmd);
            }).ok_or_else(|| format_err!("{:?}", handle))
    }

    /// Delete mesh object.
    #[inline]
    pub fn delete_mesh(&self, handle: MeshHandle) {
        self.state.meshes.delete(handle);
    }
}

impl VideoSystem {
    /// Create texture object. A texture is an image loaded in video memory,
    /// which can be sampled in shaders.
    pub fn create_texture<T>(
        &self,
        params: TextureParams,
        data: T,
    ) -> ::errors::Result<TextureHandle>
    where
        T: Into<Option<TextureData>>,
    {
        let handle = self.state.textures.create((params, data.into()))?;
        Ok(handle)
    }

    /// Creates a texture object from file asynchronously.
    pub fn create_texture_from<T: AsRef<str>>(&self, url: T) -> ::errors::Result<TextureHandle> {
        let handle = self.state.textures.create_from(url)?;
        Ok(handle)
    }

    /// Creates a texture object from file asynchronously.
    pub fn create_texture_from_uuid(&self, uuid: Uuid) -> ::errors::Result<TextureHandle> {
        let handle = self.state.textures.create_from_uuid(uuid)?;
        Ok(handle)
    }

    /// Update a contiguous subregion of an existing two-dimensional texture object.
    pub fn update_texture(
        &self,
        handle: TextureHandle,
        area: Aabb2<u32>,
        data: &[u8],
    ) -> ::errors::Result<()> {
        self.state
            .textures
            .get(handle, |_| {
                let mut frame = self.state.frames.write();
                let ptr = frame.bufs.extend_from_slice(data);
                let cmd = Command::UpdateTexture(handle, area, ptr);
                frame.cmds.push(cmd);
            }).ok_or_else(|| format_err!("{:?}", handle))
    }

    /// Delete the texture object.
    pub fn delete_texture(&self, handle: TextureHandle) {
        self.state.textures.delete(handle);
    }
}

impl VideoSystem {
    /// Create render texture object, which could be attached with a framebuffer.
    pub fn create_render_texture(
        &self,
        params: RenderTextureParams,
    ) -> Result<RenderTextureHandle> {
        let handle = self.state.render_textures.write().unwrap().create(params);

        {
            let cmd = Command::CreateRenderTexture(handle, params);
            self.state.frames.write().cmds.push(cmd);
        }

        Ok(handle)
    }

    /// Gets the `RenderTextureParams` if available.
    pub fn render_texture(&self, handle: RenderTextureHandle) -> Option<RenderTextureParams> {
        self.state
            .render_textures
            .read()
            .unwrap()
            .get(handle)
            .cloned()
    }

    /// Delete the render texture object.
    pub fn delete_render_texture(&self, handle: RenderTextureHandle) {
        if self
            .state
            .render_textures
            .write()
            .unwrap()
            .free(handle)
            .is_some()
        {
            let cmd = Command::DeleteRenderTexture(handle);
            self.state.frames.write().cmds.push(cmd);
        }
    }
}

fn dimensions_pixels() -> Vector2<u32> {
    let dimensions = crate::window::dimensions();
    let dpr = crate::window::device_pixel_ratio();
    Vector2::new(
        (dimensions.x as f32 * dpr) as u32,
        (dimensions.y as f32 * dpr) as u32,
    )
}
