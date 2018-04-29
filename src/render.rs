use std::borrow::Cow;

use arrayvec;

use rusttype;
use rusttype::{point, vector, PositionedGlyph};
use rusttype::gpu_cache::Cache;

use glium;
use glium::Surface;

use ui::Point;

pub struct DisplayList {
    rects: Vec<Rect>,
    glyphs: Vec<PositionedGlyph<'static>>,
}

impl DisplayList {
    pub fn new() -> DisplayList {
        DisplayList {
            rects: vec![],
            glyphs: vec![],
        }
    }

    pub fn merge(&mut self, other: DisplayList) {
        self.rects.extend(other.rects);
        self.glyphs.extend(other.glyphs);
    }

    pub fn translate(&mut self, delta: Point) {
        for rect in self.rects.iter_mut() {
            rect.x += delta.x;
            rect.y += delta.y;
        }

        for glyph in self.glyphs.iter_mut() {
            let old_glyph = glyph.clone();
            let position = old_glyph.position();
            *glyph = old_glyph.into_unpositioned().positioned(position + vector(delta.x, delta.y));
        }
    }

    pub fn rect(&mut self, rect: Rect) {
        self.rects.push(rect);
    }

    pub fn glyph(&mut self, glyph: PositionedGlyph<'static>) {
        self.glyphs.push(glyph);
    }
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [f32; 4],
}

pub struct Renderer {
    display: glium::Display,
    width: u32,
    height: u32,
    dpi_factor: f32,

    rect_program: glium::Program,

    cache: Cache,
    cache_tex: glium::texture::Texture2d,
    text_program: glium::Program,
}

impl Renderer {
    pub fn new(display: glium::Display, width: u32, height: u32, dpi_factor: f32) -> Renderer {
        /* initialize rect rendering */
        let rect_program = glium::Program::from_source(&display, include_str!("shader/rect_vert.glsl"), include_str!("shader/rect_frag.glsl"), None).unwrap();

        /* initialize text rendering */
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

            cache: cache,
            cache_tex: cache_tex,
            text_program: text_program,
        }
    }

    pub fn render(&mut self, display_list: DisplayList) {
        let mut target = self.display.draw();
        target.clear_color(0.01, 0.015, 0.02, 1.0);

        self.render_rects(&mut target, display_list.rects);
        self.render_glyphs(&mut target, display_list.glyphs);

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

    fn render_glyphs<'a>(&mut self, target: &mut glium::Frame, glyphs: Vec<PositionedGlyph<'a>>) {
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

    pub fn get_display(&mut self) -> &mut glium::Display {
        &mut self.display
    }

    fn pixel_to_ndc(&self, x: f32, y: f32) -> (f32, f32) {
        (2.0 * (x / self.width as f32 - 0.5), 2.0 * (1.0 - y / self.height as f32 - 0.5))
    }
}
