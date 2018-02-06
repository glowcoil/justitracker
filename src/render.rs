use std::borrow::Cow;

use arrayvec;

use rusttype;
use rusttype::{FontCollection, Font, Scale, point, vector, PositionedGlyph};
use rusttype::gpu_cache::Cache;

use glium;
use glium::Surface;

pub struct DisplayList {
    pub rects: Vec<Rect>,
    pub texts: Vec<Text>,
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
}

pub struct Text {
    pub text: String,
    pub x: f32,
    pub y: f32,
}

pub struct Renderer<'a> {
    display: glium::Display,
    width: u32,
    height: u32,
    dpi_factor: f32,

    rect_program: glium::Program,

    font: Font<'a>,
    cache: Cache,
    cache_tex: glium::texture::Texture2d,
    text_program: glium::Program,
}

impl<'a> Renderer<'a> {
    pub fn new(display: glium::Display, width: u32, height: u32, dpi_factor: f32) -> Renderer<'a> {
        /* initialize rect rendering */
        let rect_program = glium::Program::from_source(&display, include_str!("shader/rect_vert.glsl"), include_str!("shader/rect_frag.glsl"), None).unwrap();

        /* initialize text rendering */
        let collection = FontCollection::from_bytes(include_bytes!("../EPKGOBLD.TTF") as &[u8]);
        let font = collection.into_font().unwrap();

        let (cache_width, cache_height) = (512 * dpi_factor as u32, 512 * dpi_factor as u32);
        let cache = Cache::new(cache_width, cache_height, 0.1, 0.1);

        let cache_tex = glium::texture::Texture2d::with_format(
            &display,
            glium::texture::RawImage2d {
                data: Cow::Owned(vec![128u8; cache_width as usize * cache_height as usize]),
                width: cache_width,
                height: cache_height,
                format: glium::texture::ClientFormat::U8
            },
            glium::texture::UncompressedFloatFormat::U8,
            glium::texture::MipmapsOption::NoMipmap).unwrap();

        let text_program = program!(
            &display,
            140 => {
                vertex: include_str!("shader/text_vert.glsl"),
                fragment: include_str!("shader/text_frag.glsl"),
            }).unwrap();

        Renderer {
            display: display,
            width: width,
            height: height,
            dpi_factor: dpi_factor,

            rect_program: rect_program,

            font: font,
            cache: cache,
            cache_tex: cache_tex,
            text_program: text_program,
        }
    }

    pub fn render(&mut self, display_list: DisplayList) {
        let mut target = self.display.draw();
        target.clear_color(0.0, 0.03, 0.1, 1.0);

        self.render_rects(&mut target, display_list.rects);
        self.render_texts(&mut target, display_list.texts);

        target.finish().unwrap();
    }

    fn render_rects(&self, target: &mut glium::Frame, rects: Vec<Rect>) {
        #[derive(Copy, Clone)]
        struct Vertex {
            position: [f32; 2],
            color: [f32; 4],
        }
        implement_vertex!(Vertex, position, color);

        let vertices: Vec<Vertex> = rects.iter().flat_map(|r| {
            let (x1, y1) = self.pixel_to_ndc(r.x, r.y);
            let (x2, y2) = self.pixel_to_ndc(r.x + r.w, r.y + r.h);

            arrayvec::ArrayVec::<[Vertex; 6]>::from([
                Vertex { position: [x1, y1], color: r.color },
                Vertex { position: [x2, y1], color: r.color },
                Vertex { position: [x2, y2], color: r.color },
                Vertex { position: [x2, y2], color: r.color },
                Vertex { position: [x1, y2], color: r.color },
                Vertex { position: [x1, y1], color: r.color },
            ])
        }).collect();

        let vertex_buffer = glium::VertexBuffer::new(&self.display, &vertices).unwrap();

        target.draw(
            &vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            &self.rect_program,
            &glium::uniforms::EmptyUniforms,
            &Default::default()).unwrap();
    }

    fn render_texts(&mut self, target: &mut glium::Frame, texts: Vec<Text>) {
        let font = &self.font;
        let glyphs: Vec<PositionedGlyph> = texts.iter().flat_map(|t| {
            layout_paragraph(font, Scale::uniform(14.0 * self.dpi_factor), t.x, t.y, self.width, &t.text)
        }).collect();
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
                    let gl_rect = rusttype::Rect {
                        min: origin +
                            (vector(screen_rect.min.x as f32 / screen_width - 0.5,
                                      1.0 - screen_rect.min.y as f32 / screen_height - 0.5)) * 2.0,
                        max: origin +
                            (vector(screen_rect.max.x as f32 / screen_width - 0.5,
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
                &self.display,
                &vertices).unwrap()
        };

        target.draw(&vertex_buffer,
            glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList),
            &self.text_program, &uniforms,
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                ..Default::default()
            }).unwrap();
    }

    fn pixel_to_ndc(&self, x: f32, y: f32) -> (f32, f32) {
        (2.0 * (x / self.width as f32 - 0.5), 2.0 * (1.0 - y / self.height as f32 - 0.5))
    }
}

fn layout_paragraph<'a>(font: &'a Font,
                        scale: Scale,
                        x: f32,
                        y: f32,
                        width: u32,
                        text: &str) -> Vec<PositionedGlyph<'a>> {
    use unicode_normalization::UnicodeNormalization;
    let mut result = Vec::new();
    let v_metrics = font.v_metrics(scale);
    let advance_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
    let mut caret = point(x, y + v_metrics.ascent);
    let mut last_glyph_id = None;
    for c in text.nfc() {
        if c.is_control() {
            match c {
                '\r' => {
                    caret = point(x, caret.y + advance_height);
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
                caret = point(x, caret.y + advance_height);
                glyph = glyph.into_unpositioned().positioned(caret);
                last_glyph_id = None;
            }
        }
        caret.x += glyph.unpositioned().h_metrics().advance_width;
        result.push(glyph);
    }
    result
}
