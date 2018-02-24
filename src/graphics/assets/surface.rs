//! Named bucket of draw calls with the wrapping of rendering operations to a render
//! target, clearing, MSAA resolving and so on.

use utils::Color;
use graphics::MAX_FRAMEBUFFER_ATTACHMENTS;
use graphics::assets::texture::RenderTextureHandle;
use graphics::errors::*;

/// The setup data of `Surface` which wraps common rendering operations to a render-target.
/// Likes clearing, MSAA resolves, etc.. The `RenderTarget` is the window framebuffer as
/// default, but you can specify `RenderTarget` with `SurfaceSetup::set_attachments`
/// manually also.
///
/// It also plays as the named bucket of draw commands. Drawcalls inside `Surface` are
/// sorted before submitting to underlaying OpenGL. In case where order has to be
/// preserved (for example in rendering GUIs), view can be set to be in sequential order.
/// Sequential order is less efficient, because it doesn't allow state change optimization,
/// and should be avoided when possible.
///
#[derive(Debug, Copy, Clone)]
pub struct SurfaceSetup {
    pub(crate) colors: [Option<RenderTextureHandle>; MAX_FRAMEBUFFER_ATTACHMENTS],
    pub(crate) depth_stencil: Option<RenderTextureHandle>,

    pub(crate) clear_color: Option<Color>,
    pub(crate) clear_depth: Option<f32>,
    pub(crate) clear_stencil: Option<i32>,
    pub(crate) order: u64,
    pub(crate) sequence: bool,
}

impl Default for SurfaceSetup {
    fn default() -> Self {
        SurfaceSetup {
            colors: [None; MAX_FRAMEBUFFER_ATTACHMENTS],
            depth_stencil: None,
            clear_color: Some(Color::black()),
            clear_depth: Some(1.0),
            clear_stencil: None,
            sequence: false,
            order: 0,
        }
    }
}

impl_handle!(SurfaceHandle);

impl SurfaceSetup {
    /// Sets the attachments of internal frame-buffer. It consists of multiple color attachments
    /// and a optional `Depth/DepthStencil` buffer attachment.
    ///
    /// If none attachment is assigned, the default framebuffer generated by the system will be
    /// used.
    pub fn set_attachments<T1>(
        &mut self,
        colors: &[RenderTextureHandle],
        depth_stencil: T1,
    ) -> Result<()>
    where
        T1: Into<Option<RenderTextureHandle>>,
    {
        if colors.len() >= MAX_FRAMEBUFFER_ATTACHMENTS {
            return Err(Error::TooManyColorAttachments);
        }

        for (i, v) in self.colors.iter_mut().enumerate() {
            if i < colors.len() {
                *v = Some(colors[i]);
            } else {
                *v = None;
            }
        }

        self.depth_stencil = depth_stencil.into();
        Ok(())
    }

    /// By defaults, surface are sorted in ascending oreder by ids when rendering.
    /// For dynamic renderers where order might not be known until the last moment,
    /// surface ids can be remaped to arbitrary `order`.
    #[inline]
    pub fn set_order(&mut self, order: u64) {
        self.order = order;
    }

    /// Sets the clear flags for this surface.A
    #[inline]
    pub fn set_clear<C, D, S>(&mut self, color: C, depth: D, stentil: S)
    where
        C: Into<Option<Color>>,
        D: Into<Option<f32>>,
        S: Into<Option<i32>>,
    {
        self.clear_color = color.into();
        self.clear_depth = depth.into();
        self.clear_stencil = stentil.into();
    }

    /// Sets the sequence mode enable.
    ///
    /// Drawcalls inside `Surface` are sorted before submitting to underlaying OpenGL as
    /// default. In case where order has to be preserved (for example in rendering GUIs),
    /// `Surface` can be set to be in sequential order.
    ///
    /// Sequential order is less efficient, because it doesn't allow state change
    /// optimization, and should be avoided when possible.
    #[inline]
    pub fn set_sequence(&mut self, sequence: bool) {
        self.sequence = sequence;
    }
}

/// Defines a rectangle, called the scissor box, in window coordinates. The test is
/// initially disabled. While the test is enabled, only pixels that lie within the
/// scissor box can be modified by drawing commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scissor {
    Enable((u16, u16), (u16, u16)),
    Disable,
}

/// Sets the viewport of surface. This specifies the affine transformation of (x, y),
/// in window coordinates to normalized window coordinates.
/// NDC(normalized device coordinates) to normalized window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub position: (u16, u16),
    pub size: (u16, u16),
}
