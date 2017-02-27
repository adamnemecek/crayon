//! The backend of renderer, which should be responsible for only one
//! thing: submitting draw-calls using low-level OpenGL graphics APIs.

pub mod errors;
pub mod capabilities;
pub mod device;
pub mod visitor;

pub use self::errors::*;
pub use self::device::Device;
pub use self::capabilities::{Capabilities, Version, Profile};

use std::sync::{Arc, RwLock};
use gl;
use glutin;
use super::{ViewHandle, PipelineHandle, FrameBufferHandle, VertexBufferHandle, IndexBufferHandle,
            TextureHandle, RenderBufferHandle};

pub struct Context {
    window: Arc<glutin::Window>,
    context_lost: RwLock<bool>,
    capabilities: Capabilities,
    device: device::Device,
}

impl Context {
    pub fn new(window: Arc<glutin::Window>) -> Result<Self> {
        unsafe {
            window.make_current()?;
            gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);

            let capabilities = Capabilities::parse()?;
            Context::check_minimal_requirements(&capabilities)?;

            let context = Context {
                window: window,
                context_lost: RwLock::new(false),
                capabilities: capabilities,
                device: device::Device::new(),
            };

            Ok(context)
        }
    }

    fn check_minimal_requirements(caps: &Capabilities) -> Result<()> {
        if caps.version < Version::GL(1, 5) && caps.version < Version::ES(2, 0) &&
           (!caps.extensions.gl_arb_vertex_buffer_object ||
            !caps.extensions.gl_arb_map_buffer_range) {
            bail!("OpenGL implementation doesn't support vertex buffer objects.");
        }

        if caps.version < Version::GL(2, 0) && caps.version < Version::ES(2, 0) &&
           (!caps.extensions.gl_arb_shader_objects || !caps.extensions.gl_arb_vertex_shader ||
            !caps.extensions.gl_arb_fragment_shader) {
            bail!("OpenGL implementation doesn't support vertex/fragment shaders.");
        }

        if caps.version < Version::GL(3, 0) && caps.version < Version::ES(2, 0) &&
           !caps.extensions.gl_ext_framebuffer_object &&
           !caps.extensions.gl_arb_framebuffer_object {
            bail!("OpenGL implementation doesn't support framebuffers.");
        }

        if caps.version < Version::ES(2, 0) && caps.version < Version::GL(3, 0) &&
           !caps.extensions.gl_ext_framebuffer_blit {
            bail!("OpenGL implementation doesn't support blitting framebuffers.");
        }

        if caps.version < Version::GL(3, 1) && caps.version < Version::ES(3, 0) &&
           !caps.extensions.gl_arb_uniform_buffer_object {
            bail!("OpenGL implementation doesn't support uniform buffer object.");
        }

        if caps.version < Version::GL(3, 0) && caps.version < Version::ES(3, 0) &&
           !caps.extensions.gl_arb_vertex_array_object &&
           !caps.extensions.gl_apple_vertex_array_object &&
           !caps.extensions.gl_oes_vertex_array_object {
            bail!("OpenGL implementation doesn't support vertex array object.");
        }

        Ok(())
    }
}

impl Context {
    /// TODO
    pub fn rebuild(_: Arc<glutin::Window>) -> Result<()> {
        Ok(())
    }

    /// Returns the implementation of device.
    pub fn device(&mut self) -> &mut Device {
        &mut self.device
    }

    /// Returns the capabilities of this OpenGL implementation.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Returns true if the context has been lost and needs to be rebuild.
    pub fn is_context_lost(&self) -> bool {
        *self.context_lost.read().unwrap()
    }

    /// Returns true if this context is the current one in this thread.
    pub fn is_current(&self) -> bool {
        self.window.is_current()
    }

    /// Set the context as the active context in this thread.
    pub fn make_current(&self) -> Result<()> {
        unsafe { self.window.make_current().chain_err(|| "unable to make context current.") }
    }

    /// Swaps the buffers in case of double or triple buffering.
    ///
    /// **Warning**: if you enabled vsync, this function will block until the
    /// next time the screen is refreshed. However drivers can choose to
    /// override your vsync settings, which means that you can't know in advance
    /// whether swap_buffers will block or not.
    pub fn swap_buffers(&self) -> Result<()> {
        if *self.context_lost.read().unwrap() {
            bail!(ErrorKind::ContextLost);
        }

        match self.window.swap_buffers() {
            Err(glutin::ContextError::ContextLost) => {
                *self.context_lost.write().unwrap() = true;
                bail!(ErrorKind::ContextLost);
            }
            other => other.chain_err(|| "unable to swap buffers."),
        }
    }
}