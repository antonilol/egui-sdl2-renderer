use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::mem::offset_of;

use egui::epaint::{ImageDelta, Primitive, Vertex};
use egui::{Color32, PaintCallbackInfo, TextureId};
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{
    Canvas, RenderGeometryTextureParams, Texture, TextureAccess, TextureCreator, TextureValueError,
    UpdateTextureError,
};
use sdl2::video::{Window, WindowContext};

pub use sdl2;

#[derive(Debug, Clone)]
pub enum PainterError {
    SdlRenderGeometryUnsupported,
    SdlError(String),
    UpdateTexture(UpdateTextureError),
    CreateTexture(TextureValueError),
    FreeInvalidTexture(TextureId),
    PaintInvalidTexture(TextureId),
    BlendModeNotSupported,
}

impl From<UpdateTextureError> for PainterError {
    fn from(value: UpdateTextureError) -> Self {
        Self::UpdateTexture(value)
    }
}

impl From<TextureValueError> for PainterError {
    fn from(value: TextureValueError) -> Self {
        Self::CreateTexture(value)
    }
}

impl fmt::Display for PainterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SdlRenderGeometryUnsupported => {
                write!(f, "SDL_RenderGeometry not supported")
            }
            Self::SdlError(err) => {
                write!(f, "SDL error: {err}")
            }
            Self::UpdateTexture(err) => {
                write!(f, "unable to update texture: {err}")
            }
            Self::CreateTexture(err) => {
                write!(f, "unable to create texture: {err}")
            }
            Self::FreeInvalidTexture(id) => {
                write!(f, "unable to free texture {id:?}: texture does not exist")
            }
            Self::PaintInvalidTexture(id) => {
                write!(
                    f,
                    "unable to paint using texture {id:?}: texture does not exist",
                )
            }
            Self::BlendModeNotSupported => {
                write!(f, "blend mode needed by egui not supported")
            }
        }
    }
}

impl std::error::Error for PainterError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::UpdateTexture(err) => Some(err),
            Self::CreateTexture(err) => Some(err),
            Self::SdlRenderGeometryUnsupported
            | Self::SdlError(_)
            | Self::FreeInvalidTexture(_)
            | Self::PaintInvalidTexture(_)
            | Self::BlendModeNotSupported => None,
        }
    }
}

pub struct CallbackFn {
    #[expect(clippy::type_complexity)]
    f: Box<dyn Fn(PaintCallbackInfo, &Painter, &mut Canvas<Window>) + Sync + Send>,
}

impl CallbackFn {
    pub fn new<F: Fn(PaintCallbackInfo, &Painter, &mut Canvas<Window>) + Sync + Send + 'static>(
        callback: F,
    ) -> Self {
        Self {
            f: Box::new(callback),
        }
    }
}

pub struct Painter<'texture> {
    texture_creator: &'texture TextureCreator<WindowContext>,
    // TODO rustc-hash?
    textures: HashMap<TextureId, Texture<'texture>>,
}

impl<'texture> Painter<'texture> {
    pub fn new(texture_creator: &'texture TextureCreator<WindowContext>) -> Self {
        Self {
            texture_creator,
            textures: HashMap::new(),
        }
    }

    fn update_or_create_texture(
        &mut self,
        id: TextureId,
        delta: &ImageDelta,
    ) -> Result<(), PainterError> {
        use sdl2::sys::{SDL_BlendFactor, SDL_BlendOperation, SDL_ComposeCustomBlendMode};

        // TODO use safe binding coming in sdl2 v0.39 (https://github.com/Rust-SDL2/rust-sdl2/pull/1507)
        let blend_mode = unsafe {
            SDL_ComposeCustomBlendMode(
                SDL_BlendFactor::SDL_BLENDFACTOR_ONE,
                SDL_BlendFactor::SDL_BLENDFACTOR_ONE_MINUS_SRC_ALPHA,
                SDL_BlendOperation::SDL_BLENDOPERATION_ADD,
                SDL_BlendFactor::SDL_BLENDFACTOR_ONE_MINUS_DST_ALPHA,
                SDL_BlendFactor::SDL_BLENDFACTOR_ONE,
                SDL_BlendOperation::SDL_BLENDOPERATION_ADD,
            )
        };

        let [x, y] = delta
            .pos
            .map(|pos| pos.map(|coord| coord.try_into().unwrap()))
            .unwrap_or([0, 0]);
        let width = delta.image.width().try_into().unwrap();
        let height = delta.image.height().try_into().unwrap();

        let texture = match self.textures.entry(id) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert({
                let texture = self.texture_creator.create_texture(
                    PixelFormatEnum::RGBA32,
                    TextureAccess::Static,
                    width,
                    height,
                )?;

                let ret = unsafe { sdl2::sys::SDL_SetTextureBlendMode(texture.raw(), blend_mode) };
                if ret < 0 {
                    return Err(PainterError::BlendModeNotSupported);
                }

                texture
            }),
        };

        let egui::ImageData::Color(image) = &delta.image;

        assert_eq!(
            image.width() * image.height(),
            image.pixels.len(),
            "Mismatch between texture size and texel count",
        );

        let pixels: *const [Color32] = image.pixels.as_slice();
        // SAFETY: `Color32` just wraps `[u8; 4]` and is repr(C)
        let data = unsafe { &*(pixels as *const [[u8; 4]]) }.as_flattened();

        // TODO
        // let TextureOptions { magnification, minification, wrap_mode } = delta.options;
        // filter mode can only be set for both magnification and minification
        // sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "nearest");
        // sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "linear");
        // sdl2 does not support setting wrap mode, sdl3 also does not (there is a closed pr that might be reopened)

        texture.update(
            Rect::from((x, y, width, height)),
            data,
            delta.image.width() * size_of::<Color32>(),
        )?;

        Ok(())
    }

    fn free_texture(&mut self, id: TextureId) -> bool {
        self.textures.remove(&id).is_some()
    }

    pub fn paint_and_update_textures(
        &mut self,
        canvas: &mut Canvas<Window>,
        screen_size_px: [u32; 2],
        pixels_per_point: f32,
        clipped_primitives: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
    ) -> Result<(), PainterError> {
        for (id, delta) in &textures_delta.set {
            self.update_or_create_texture(*id, delta)?;
        }

        for p in clipped_primitives {
            match &p.primitive {
                Primitive::Mesh(mesh) => {
                    let texture = self
                        .textures
                        .get(&mesh.texture_id)
                        .ok_or(PainterError::PaintInvalidTexture(mesh.texture_id))?;

                    let clip_size = p.clip_rect.size();
                    canvas.set_clip_rect(Rect::from((
                        p.clip_rect.min.x as i32,
                        p.clip_rect.min.y as i32,
                        clip_size.x as u32,
                        clip_size.y as u32,
                    )));

                    unsafe {
                        canvas.render_geometry_raw(
                            &mesh.vertices,
                            offset_of!(Vertex, pos),
                            &mesh.vertices,
                            offset_of!(Vertex, color),
                            Some(RenderGeometryTextureParams {
                                texture,
                                tex_coords: &mesh.vertices,
                                tex_coord_offset: offset_of!(Vertex, uv),
                            }),
                            &mesh.indices,
                        )
                    }
                    .map_err(PainterError::SdlError)?;
                }
                Primitive::Callback(paint_callback) => {
                    let info = egui::PaintCallbackInfo {
                        viewport: paint_callback.rect,
                        clip_rect: p.clip_rect,
                        pixels_per_point,
                        screen_size_px,
                    };

                    if let Some(callback) = paint_callback.callback.downcast_ref::<CallbackFn>() {
                        (callback.f)(info, self, canvas);
                    } else {
                        // eprintln!("invalid callback, expected egui_sdl2_renderer::CallbackFn");
                    }
                }
            }
        }

        for &id in &textures_delta.free {
            if !self.free_texture(id) {
                return Err(PainterError::FreeInvalidTexture(id));
            }
        }

        Ok(())
    }
}
