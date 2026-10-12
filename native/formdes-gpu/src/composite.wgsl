// formdes's frame shader: one instanced quad per shadow, page and tile, in device pixels. Each instance carries
//   rect   : x, y, w, h of the quad on the target
//   aux    : a tile's uv rectangle (u0, v0, u1, v1); a shadow's inner rectangle (x, y, w, h)
//   bounds : the uv range a tile may sample, kept half a texel inside its pixels so filtering never reads past an edge tile
//   params : a shadow's blur sigma and opacity
// Everything is blended premultiplied, source over.
struct Frame { size: vec4<f32> };   // target width, height, device pixel ratio, unused
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var tile: texture_2d<f32>;
@group(1) @binding(1) var smp: sampler;

struct Inst {
  @location(0) rect: vec4<f32>,
  @location(1) aux: vec4<f32>,
  @location(2) bounds: vec4<f32>,
  @location(3) params: vec4<f32>,
};
struct Out {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) @interpolate(flat) aux: vec4<f32>,
  @location(2) @interpolate(flat) bounds: vec4<f32>,
  @location(3) @interpolate(flat) params: vec4<f32>,
};

@vertex fn vs(@builtin(vertex_index) i: u32, inst: Inst) -> Out {
  let c = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
  let t = c[i];
  let px = inst.rect.xy + t * inst.rect.zw;
  var o: Out;
  o.pos = vec4<f32>(px.x / frame.size.x * 2.0 - 1.0, 1.0 - px.y / frame.size.y * 2.0, 0.0, 1.0);
  o.uv = mix(inst.aux.xy, inst.aux.zw, t);
  o.aux = inst.aux;
  o.bounds = inst.bounds;
  o.params = inst.params;
  return o;
}

// A page's paper: opaque white, drawn under its tiles so a tile that is not on the GPU yet leaves paper, not a hole.
@fragment fn fs_page(in: Out) -> @location(0) vec4<f32> {
  return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

// A page's drop shadow: the inner rectangle blurred, as a smooth step across each edge in each axis.
@fragment fn fs_shadow(in: Out) -> @location(0) vec4<f32> {
  let p = in.pos.xy;
  let sigma = max(in.params.x, 1e-3);
  let dx = min(p.x - in.aux.x, in.aux.x + in.aux.z - p.x);
  let dy = min(p.y - in.aux.y, in.aux.y + in.aux.w - p.y);
  let a = smoothstep(-sigma, sigma, dx) * smoothstep(-sigma, sigma, dy) * in.params.y;
  return vec4<f32>(0.0, 0.0, 0.0, a);
}

// A tile's pixels are straight RGBA; premultiplied here for the blend.
@fragment fn fs_tile(in: Out) -> @location(0) vec4<f32> {
  let uv = clamp(in.uv, in.bounds.xy, in.bounds.zw);
  let c = textureSample(tile, smp, uv);
  return vec4<f32>(c.rgb * c.a, c.a);
}
