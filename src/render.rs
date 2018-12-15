use std::borrow::Cow;

use arrayvec;

use rusttype;
use rusttype::{point, vector, PositionedGlyph};
use rusttype::gpu_cache::Cache;

use glium;
use glium::Surface;

use ui::{Point, BoundingBox, Overlap};

struct DisplayListItems {
    rects: Vec<Rect>,
    glyphs: Vec<PositionedGlyph<'static>>,
}

impl DisplayListItems {
    fn new() -> DisplayListItems {
        DisplayListItems {
            rects: Vec::new(),
            glyphs: Vec::new(),
        }
    }
}

pub struct DisplayList {
    items: DisplayListItems,
    clip_rects: Vec<(BoundingBox, DisplayListItems)>,
    translate_stack: Vec<Point>,
    clip_rect_stack: Vec<(usize, BoundingBox)>,
    context_stack: Vec<DisplayListContext>,
}

impl DisplayList {
    pub fn new() -> DisplayList {
        DisplayList {
            items: DisplayListItems::new(),
            clip_rects: Vec::new(),
            translate_stack: vec![],
            clip_rect_stack: Vec::new(),
        }
    }

    pub fn push_translate(&mut self, delta: Point) {
        let mut delta = delta;
        if let Some(top_delta) = self.translate_stack.last() {
            delta += *top_delta;
        }
        self.translate_stack.push(delta);
    }

    pub fn pop_translate(&mut self) {
        self.translate_stack.pop();
    }

    pub fn push_clip_rect(&mut self, clip_rect: BoundingBox) {
        let mut clip_rect = clip_rect;
        if let Some(top_delta) = self.translate_stack.last() {
            clip_rect.pos += *top_delta;
        }
        if let Some((_, top_rect)) = self.clip_rect_stack.last() {
            clip_rect.pos.x = clip_rect.pos.x.max(top_rect.pos.x);
            clip_rect.pos.y = clip_rect.pos.y.max(top_rect.pos.y);
            clip_rect.size.x = clip_rect.size.x.min(top_rect.size.x);
            clip_rect.size.y = clip_rect.size.y.min(top_rect.size.y);
        }
        self.clip_rect_stack.push((self.clip_rects.len(), clip_rect));
        self.clip_rects.push((clip_rect, DisplayListItems::new()));
    }

    pub fn pop_clip_rect(&mut self) {
        self.clip_rect_stack.pop();
    }

    pub fn rect(&mut self, rect: Rect) {
        let mut rect = rect;
        if let Some(delta) = self.translate_stack.last() {
            rect.bounds.pos.x += delta.x;
            rect.bounds.pos.y += delta.y;
        }
        if let Some((i, clip_rect)) = self.clip_rect_stack.last() {
            // match rect.bounds.overlaps(clip_rect) {
            //     Overlap::Inside => { self.items.rects.push(rect); }
            //     Overlap::Overlap => {
                    self.clip_rects[*i].1.rects.push(rect);
            //     }
            //     Overlap::Outside => { /* don't draw */ }
            // }
        } else {
            self.items.rects.push(rect);
        }
    }

    pub fn glyph(&mut self, glyph: PositionedGlyph<'static>) {
        let mut glyph = glyph.clone();
        if let Some(delta) = self.translate_stack.last() {
            let position = glyph.position();
            glyph = glyph.into_unpositioned().positioned(point(position.x + delta.x, position.y + delta.y));
        }
        if let Some((i, clip_rect)) = self.clip_rect_stack.last() {
            // if let Some(bbox) = glyph.pixel_bounding_box() {
            //     let bounds = BoundingBox::new(bbox.min.x as f32, bbox.min.y as f32, bbox.max.x as f32, bbox.max.y as f32);
            //     match bounds.overlaps(clip_rect) {
            //         Overlap::Inside => { self.items.glyphs.push(glyph); }
            //         Overlap::Overlap => { self.clip_rects[*i].1.glyphs.push(glyph); }
            //         Overlap::Outside => { /* don't draw */ }
            //     }
            // } else {
                self.clip_rects[*i].1.glyphs.push(glyph);
            // }
        } else {
            self.items.glyphs.push(glyph);
        }
    }
}

pub struct Rect {
    pub bounds: BoundingBox,
    pub color: [f32; 4],
}

pub struct Renderer {
    display: glium::Display,
    width: f32,
    height: f32,
    dpi_factor: f32,

    rect_program: glium::Program,

    cache: Cache,
    cache_tex: glium::texture::Texture2d,
    text_program: glium::Program,
}

impl Renderer {
    pub fn new(display: glium::Display, width: f32, height: f32, dpi_factor: f32) -> Renderer {
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

        self.render_rects(&mut target, &display_list.items.rects, None);
        self.render_glyphs(&mut target, &display_list.items.glyphs, None);
        for (scissor, items) in display_list.clip_rects.iter() {
            self.render_rects(&mut target, &items.rects, Some(*scissor));
            self.render_glyphs(&mut target, &items.glyphs, Some(*scissor));
        }

        target.finish().unwrap();
    }

    fn render_rects(&self, target: &mut glium::Frame, rects: &[Rect], scissor: Option<BoundingBox>) {
        #[derive(Copy, Clone)]
        struct Vertex {
            position: [f32; 2],
            color: [f32; 4],
        }
        implement_vertex!(Vertex, position, color);

        let vertices: Vec<Vertex> = rects.iter().flat_map(|r| {
            let (x1, y1) = self.pixel_to_ndc(r.bounds.pos.x, r.bounds.pos.y);
            let (x2, y2) = self.pixel_to_ndc(r.bounds.pos.x + r.bounds.size.x, r.bounds.pos.y + r.bounds.size.y);

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
            &glium::DrawParameters {
                blend: glium::Blend::alpha_blending(),
                scissor: scissor.map(|bounds| glium::Rect { left: bounds.pos.x as u32, bottom: (self.height - bounds.pos.y - bounds.size.y) as u32, width: bounds.size.x as u32, height: bounds.size.y as u32 }),
                ..Default::default()
            }).unwrap();
    }

    fn render_glyphs<'a>(&mut self, target: &mut glium::Frame, glyphs: &[PositionedGlyph<'a>], scissor: Option<BoundingBox>) {
        for glyph in glyphs {
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
                scissor: scissor.map(|bounds| glium::Rect { left: bounds.pos.x as u32, bottom: (self.height - bounds.pos.y - bounds.size.y) as u32, width: bounds.size.x as u32, height: bounds.size.y as u32 }),
                ..Default::default()
            }).unwrap();
    }

    pub fn get_display(&mut self) -> &mut glium::Display {
        &mut self.display
    }

    fn pixel_to_ndc(&self, x: f32, y: f32) -> (f32, f32) {
        let (screen_width, screen_height) = {
            let (w, h) = self.display.get_framebuffer_dimensions();
            (w as f32, h as f32)
        };
        (2.0 * (x / screen_width as f32 - 0.5), 2.0 * (1.0 - y / screen_height as f32 - 0.5))
    }
}
