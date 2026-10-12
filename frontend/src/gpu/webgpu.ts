import { isOneToOne, place, sizeCanvas, SHADOW_ALPHA, SHADOW_BLUR, SHADOW_OFFSET, type Compositor, type Frame, type TileSource } from './index';

// WebGPU path. One rgba8 texture per tile and one instanced quad per shadow, page and tile (the layout of
// native/formdes-gpu/src/composite.wgsl): rect and uv in the instance, blended premultiplied, source over.
// The GPU types are not in this project's lib, so the objects are typed loosely.

const WGSL = /* wgsl */ `
struct Frame { size: vec4<f32> };
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

@fragment fn fs_page(in: Out) -> @location(0) vec4<f32> {
  return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

@fragment fn fs_shadow(in: Out) -> @location(0) vec4<f32> {
  let p = in.pos.xy;
  let sigma = max(in.params.x, 1e-3);
  let dx = min(p.x - in.aux.x, in.aux.x + in.aux.z - p.x);
  let dy = min(p.y - in.aux.y, in.aux.y + in.aux.w - p.y);
  let a = smoothstep(-sigma, sigma, dx) * smoothstep(-sigma, sigma, dy) * in.params.y;
  return vec4<f32>(0.0, 0.0, 0.0, a);
}

@fragment fn fs_tile(in: Out) -> @location(0) vec4<f32> {
  let uv = clamp(in.uv, in.bounds.xy, in.bounds.zw);
  let c = textureSample(tile, smp, uv);
  return vec4<f32>(c.rgb * c.a, c.a);
}
`;

const BUF = { COPY_DST: 8, VERTEX: 32, UNIFORM: 64 };
const TEX = { COPY_DST: 2, TEXTURE_BINDING: 4, RENDER_ATTACHMENT: 16 };
const STAGE = { VERTEX: 1, FRAGMENT: 2 };
const STRIDE = 16; // floats per instance

interface Entry { tex: any; view: any; w: number; h: number; linear: any; nearest: any }

export class WebGpuCompositor implements Compositor {
  readonly name = 'WebGPU';
  onLost: ((why: string) => void) | null = null;
  canvas: HTMLCanvasElement | null = null;
  private device: any = null;
  private ctx: any = null;
  private pipes: { shadow: any; page: any; tile: any } | null = null;
  private frameBind: any = null;
  private uniform: any = null;
  private tileLayout: any = null;
  private samplers: { linear: any; nearest: any } | null = null;
  private inst: any = null;
  private instCap = 0;
  private readonly tiles = new Map<string, Entry>();
  private dead = false;
  private w = 1;
  private h = 1;

  async init(canvas: HTMLCanvasElement): Promise<boolean> {
    try {
      const gpu = (navigator as any).gpu;
      if (!gpu) return false;
      const adapter = await gpu.requestAdapter();
      if (!adapter) return false;
      const device = await adapter.requestDevice();
      const ctx: any = canvas.getContext('webgpu');
      if (!ctx) { device.destroy?.(); return false; }
      const format = gpu.getPreferredCanvasFormat();
      ctx.configure({ device, format, alphaMode: 'opaque' });
      device.pushErrorScope('validation');
      const module = device.createShaderModule({ code: WGSL });
      const frameLayout = device.createBindGroupLayout({ entries: [{ binding: 0, visibility: STAGE.VERTEX, buffer: { type: 'uniform' } }] });
      this.tileLayout = device.createBindGroupLayout({
        entries: [
          { binding: 0, visibility: STAGE.FRAGMENT, texture: { sampleType: 'float' } },
          { binding: 1, visibility: STAGE.FRAGMENT, sampler: { type: 'filtering' } },
        ],
      });
      const flat = device.createPipelineLayout({ bindGroupLayouts: [frameLayout] });
      const withTile = device.createPipelineLayout({ bindGroupLayouts: [frameLayout, this.tileLayout] });
      const over = { color: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha', operation: 'add' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha', operation: 'add' } };
      const make = (layout: any, fs: string) => device.createRenderPipeline({
        layout,
        vertex: {
          module, entryPoint: 'vs',
          buffers: [{ arrayStride: STRIDE * 4, stepMode: 'instance', attributes: [0, 1, 2, 3].map((i) => ({ shaderLocation: i, offset: i * 16, format: 'float32x4' })) }],
        },
        fragment: { module, entryPoint: fs, targets: [{ format, blend: over }] },
        primitive: { topology: 'triangle-list' },
      });
      this.pipes = { shadow: make(flat, 'fs_shadow'), page: make(flat, 'fs_page'), tile: make(withTile, 'fs_tile') };
      this.uniform = device.createBuffer({ size: 16, usage: BUF.UNIFORM | BUF.COPY_DST });
      this.frameBind = device.createBindGroup({ layout: frameLayout, entries: [{ binding: 0, resource: { buffer: this.uniform } }] });
      const sampler = (f: string) => device.createSampler({ magFilter: f, minFilter: f, addressModeU: 'clamp-to-edge', addressModeV: 'clamp-to-edge' });
      this.samplers = { linear: sampler('linear'), nearest: sampler('nearest') };
      const err = await device.popErrorScope();
      if (err) { device.destroy?.(); return false; }
      this.device = device;
      this.ctx = ctx;
      this.canvas = canvas;
      device.lost.then((info: any) => {
        if (this.dead) return;
        this.dead = true;
        this.onLost?.(String(info?.reason ?? 'lost') + (info?.message ? `: ${info.message}` : ''));
      });
      return true;
    } catch {
      return false;
    }
  }

  upload(key: string, source: TileSource): void {
    const device = this.device;
    if (!device || this.dead) return;
    this.evict(key);
    const w = source.width, h = source.height;
    if (w < 1 || h < 1) return;
    try {
      const tex = device.createTexture({ size: [w, h], format: 'rgba8unorm', usage: TEX.COPY_DST | TEX.TEXTURE_BINDING | TEX.RENDER_ATTACHMENT });
      if (typeof ImageData !== 'undefined' && source instanceof ImageData) {
        device.queue.writeTexture({ texture: tex }, source.data, { bytesPerRow: w * 4 }, [w, h]);
      } else {
        device.queue.copyExternalImageToTexture({ source: source as ImageBitmap }, { texture: tex, premultipliedAlpha: false }, [w, h]);
      }
      const view = tex.createView();
      const bind = (s: any) => device.createBindGroup({ layout: this.tileLayout, entries: [{ binding: 0, resource: view }, { binding: 1, resource: s }] });
      this.tiles.set(key, { tex, view, w, h, linear: bind(this.samplers!.linear), nearest: bind(this.samplers!.nearest) });
    } catch { /* a failed upload leaves the tile missing; it will be asked for again */ }
  }

  has(key: string): boolean { return this.tiles.has(key); }

  evict(key: string): void {
    const t = this.tiles.get(key);
    if (!t) return;
    this.tiles.delete(key);
    try { t.tex.destroy(); } catch { }
  }

  draw(frame: Frame): string[] {
    const device = this.device, pipes = this.pipes;
    if (!device || !pipes || this.dead || !this.ctx) return [];
    const missing: string[] = [];
    const shadows: number[] = [], pages: number[] = [];
    const dpr = frame.dpr, blur = SHADOW_BLUR * dpr, off = SHADOW_OFFSET * dpr, reach = blur * 2;
    for (const p of frame.pages) {
      const [x0, y0, x1, y1] = place(frame, p);
      const dw = x1 - x0, dh = y1 - y0;
      const sy = y0 + off;
      if (x0 - reach >= frame.width || sy - reach >= frame.height || x1 + reach <= 0 || sy + dh + reach <= 0) continue;
      shadows.push(x0 - reach, sy - reach, dw + reach * 2, dh + reach * 2, x0, sy, dw, dh, 0, 0, 0, 0, blur, SHADOW_ALPHA, 0, 0);
      pages.push(x0, y0, dw, dh, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    }
    const drawn: Array<{ e: Entry; nearest: boolean }> = [];
    const tileData: number[] = [];
    for (const t of frame.tiles) {
      const e = this.tiles.get(t.key);
      if (!e) { missing.push(t.key); continue; }
      const [x0, y0, x1, y1] = place(frame, t.rect);
      if (x1 <= x0 || y1 <= y0) continue;
      tileData.push(x0, y0, x1 - x0, y1 - y0, 0, 0, 1, 1, 0.5 / e.w, 0.5 / e.h, 1 - 0.5 / e.w, 1 - 0.5 / e.h, 0, 0, 0, 0);
      drawn.push({ e, nearest: isOneToOne(frame, t) });
    }
    const ns = shadows.length / STRIDE, np = pages.length / STRIDE;
    const all = new Float32Array(shadows.length + pages.length + tileData.length);
    all.set(shadows, 0);
    all.set(pages, shadows.length);
    all.set(tileData, shadows.length + pages.length);
    if (all.byteLength > this.instCap) {
      this.inst?.destroy?.();
      this.instCap = Math.max(4096, 1 << Math.ceil(Math.log2(all.byteLength)));
      this.inst = device.createBuffer({ size: this.instCap, usage: BUF.VERTEX | BUF.COPY_DST });
    }
    if (all.byteLength) device.queue.writeBuffer(this.inst, 0, all);
    device.queue.writeBuffer(this.uniform, 0, new Float32Array([frame.width, frame.height, frame.dpr, 0]));
    try {
      const view = this.ctx.getCurrentTexture().createView();
      const enc = device.createCommandEncoder();
      const pass = enc.beginRenderPass({
        colorAttachments: [{ view, loadOp: 'clear', storeOp: 'store', clearValue: { r: frame.bg[0] / 255, g: frame.bg[1] / 255, b: frame.bg[2] / 255, a: 1 } }],
      });
      if (all.byteLength) {
        pass.setVertexBuffer(0, this.inst);
        pass.setBindGroup(0, this.frameBind);
        if (ns) { pass.setPipeline(pipes.shadow); pass.draw(6, ns, 0, 0); }
        if (np) { pass.setPipeline(pipes.page); pass.draw(6, np, 0, ns); }
        if (drawn.length) {
          pass.setPipeline(pipes.tile);
          drawn.forEach((d, i) => {
            pass.setBindGroup(1, d.nearest ? d.e.nearest : d.e.linear);
            pass.draw(6, 1, 0, ns + np + i);
          });
        }
      }
      pass.end();
      device.queue.submit([enc.finish()]);
    } catch { /* a lost device reports through device.lost */ }
    return missing;
  }

  resize(width: number, height: number, dpr: number): void {
    if (!this.canvas) return;
    this.w = width;
    this.h = height;
    sizeCanvas(this.canvas, width, height, dpr);
  }

  destroy(): void {
    this.dead = true;
    for (const k of [...this.tiles.keys()]) this.evict(k);
    try { this.inst?.destroy?.(); } catch { }
    try { this.ctx?.unconfigure?.(); } catch { }
    try { this.device?.destroy?.(); } catch { }
    this.device = null;
    this.pipes = null;
    this.ctx = null;
  }
}
