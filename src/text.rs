use glium;
use arrayvec;

use std::borrow::Cow;

use rusttype::{FontCollection, Font, Scale, point, vector, PositionedGlyph, Rect};
use rusttype::gpu_cache::Cache;

use glium::Surface;

pub struct TextRenderer<'a> {
    font: Font<'a>,
    cache: Cache,
    cache_tex: glium::texture::Texture2d,
    program: glium::Program,
    dpi_factor: f32,
    display: &'a glium::Display,
}

impl<'a> TextRenderer<'a> {
    pub fn new(display: &'a glium::Display, font_data: &'static [u8], width: u32, dpi_factor: f32) -> TextRenderer<'a> {
        let collection = FontCollection::from_bytes(font_data as &[u8]);
        let font = collection.into_font().unwrap();

        let (cache_width, cache_height) = (512 * dpi_factor as u32, 512 * dpi_factor as u32);
        let mut cache = Cache::new(cache_width, cache_height, 0.1, 0.1);

        let cache_tex = glium::texture::Texture2d::with_format(
            display,
            glium::texture::RawImage2d {
                data: Cow::Owned(vec![128u8; cache_width as usize * cache_height as usize]),
                width: cache_width,
                height: cache_height,
                format: glium::texture::ClientFormat::U8
            },
            glium::texture::UncompressedFloatFormat::U8,
            glium::texture::MipmapsOption::NoMipmap).unwrap();

        let program = program!(
            display,
            140 => {
                vertex: "
                    #version 140

                    in vec2 position;
                    in vec2 tex_coords;
                    in vec4 colour;

                    out vec2 v_tex_coords;
                    out vec4 v_colour;

                    void main() {
                        gl_Position = vec4(position, 0.0, 1.0);
                        v_tex_coords = tex_coords;
                        v_colour = colour;
                    }
                ",

                fragment: "
                    #version 140
                    uniform sampler2D tex;
                    in vec2 v_tex_coords;
                    in vec4 v_colour;
                    out vec4 f_colour;

                    void main() {
                        f_colour = v_colour * texture(tex, v_tex_coords).rrrr;
                    }
                "
            }).unwrap();

        TextRenderer {
            font: font,
            cache: cache,
            cache_tex: cache_tex,
            program: program,
            dpi_factor: dpi_factor,
            display: display,
        }
    }

    pub fn draw(&mut self, target: &mut glium::Frame, width: u32, text: &str) {
        let glyphs = layout_paragraph(&self.font, Scale::uniform(14.0 * self.dpi_factor), width, &text);
        for glyph in &glyphs {
            self.cache.queue_glyph(0, glyph.clone());
        }
        {
            let cache_tex = &mut self.cache_tex;
            self.cache.cache_queued(|rect, data| {
                cache_tex.main_level().write(glium::Rect {
                    left: rect.min.x,
                    bottom: rect.min.y,
                    width: rect.width(),
                    height: rect.height()
                }, glium::texture::RawImage2d {
                    data: Cow::Borrowed(data),
                    width: rect.width(),
                    height: rect.height(),
                    format: glium::texture::ClientFormat::U8
                });
            }).unwrap();
        }

        let uniforms = uniform! {
            tex: self.cache_tex.sampled().magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
        };

        let vertex_buffer = {
            #[derive(Copy, Clone)]
            struct Vertex {
                position: [f32; 2],
                tex_coords: [f32; 2],
                colour: [f32; 4]
            }

            implement_vertex!(Vertex, position, tex_coords, colour);
            let colour = [1.0, 1.0, 1.0, 1.0];
            let (screen_width, screen_height) = {
                let (w, h) = self.display.get_framebuffer_dimensions();
                (w as f32, h as f32)
            };
            let origin = point(0.0, 0.0);
            let vertices: Vec<Vertex> = glyphs.iter().flat_map(|g| {
                if let Ok(Some((uv_rect, screen_rect))) = self.cache.rect_for(0, g) {
                    let gl_rect = Rect {
                        min: origin
                            + (vector(screen_rect.min.x as f32 / screen_width - 0.5,
                                      1.0 - screen_rect.min.y as f32 / screen_height - 0.5)) * 2.0,
                        max: origin
                            + (vector(screen_rect.max.x as f32 / screen_width - 0.5,
                                      1.0 - screen_rect.max.y as f32 / screen_height - 0.5)) * 2.0
                    };
                    arrayvec::ArrayVec::<[Vertex; 6]>::from([
                        Vertex {
                            position: [gl_rect.min.x, gl_rect.max.y],
                            tex_coords: [uv_rect.min.x, uv_rect.max.y],
                            colour: colour
                        },
                        Vertex {
                            position: [gl_rect.min.x,  gl_rect.min.y],
                            tex_coords: [uv_rect.min.x, uv_rect.min.y],
                            colour: colour
                        },
                        Vertex {
                            position: [gl_rect.max.x,  gl_rect.min.y],
                            tex_coords: [uv_rect.max.x, uv_rect.min.y],
                            colour: colour
                        },
                        Vertex {
                            position: [gl_rect.max.x,  gl_rect.min.y],
                            tex_coords: [uv_rect.max.x, uv_rect.min.y],
                            colour: colour },
                        Vertex {
                            position: [gl_rect.max.x, gl_rect.max.y],
                            tex_coords: [uv_rect.max.x, uv_rect.max.y],
                            colour: colour
                        },
                        Vertex {
                            position: [gl_rect.min.x, gl_rect.max.y],
                            tex_coords: [uv_rect.min.x, uv_rect.max.y],
                            colour: colour
                        }])
                } else {
                    arrayvec::ArrayVec::new()
                }
            }).collect();

            glium::VertexBuffer::new(
                self.display,
                &vertices).unwrap()
        };

        target.draw(&vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            &self.program, &uniforms,
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                ..Default::default()
            }).unwrap();
    }
}

fn layout_paragraph<'a>(font: &'a Font,
                        scale: Scale,
                        width: u32,
                        text: &str) -> Vec<PositionedGlyph<'a>> {
    use unicode_normalization::UnicodeNormalization;
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut caret = point(0.0, v_metrics.ascent);
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            match c {
                '\r' => {
                    caret = point(0.0, caret.y + advance_height);
                }
                '\n' => {},
                _ => {}
            }
            continue;
        }
        let base_glyph = if let Some(glyph) = font.glyph(c) {
            glyph
        } else {
            continue;
        };
        if let Some(id) = last_glyph_id.take() {
            caret.x += font.pair_kerning(scale, id, base_glyph.id());
        }
        last_glyph_id = Some(base_glyph.id());
        let mut glyph = base_glyph.scaled(scale).positioned(caret);
        if let Some(bb) = glyph.pixel_bounding_box() {
            if bb.max.x > width as i32 {
                caret = point(0.0, caret.y + advance_height);
                glyph = glyph.into_unpositioned().positioned(caret);
                last_glyph_id = None;
            }
        }
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        result.push(glyph);
    }
    result
}
