import { isOneToOne, place, sizeCanvas, SHADOW_ALPHA, SHADOW_BLUR, SHADOW_OFFSET, type Compositor, type Frame, type TileSource } from './index';

// WebGL2 path: the same frame model as webgpu.ts in GLSL ES 3.0. WebGL2 has no base instance, so each tile draw points
// the instance attributes at its own 64 byte record; shadows and pages are drawn as one instanced batch each.

const VS = `#version 300 es
precision highp float;
layout(location = 0) in vec4 a_rect;
layout(location = 1) in vec4 a_aux;
layout(location = 2) in vec4 a_bounds;
layout(location = 3) in vec4 a_params;
uniform vec2 u_size;
out vec2 v_uv;
out vec2 v_px;
flat out vec4 v_aux;
flat out vec4 v_bounds;
flat out vec4 v_params;
void main() {
  vec2 c[6] = vec2[6](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
  vec2 t = c[gl_VertexID];
  vec2 px = a_rect.xy + t * a_rect.zw;
  gl_Position = vec4(px.x / u_size.x * 2.0 - 1.0, 1.0 - px.y / u_size.y * 2.0, 0.0, 1.0);
  v_uv = mix(a_aux.xy, a_aux.zw, t);
  v_px = px;
  v_aux = a_aux;
  v_bounds = a_bounds;
  v_params = a_params;
}`;

const FS = `#version 300 es
precision highp float;
uniform int u_mode;
uniform sampler2D u_tile;
in vec2 v_uv;
in vec2 v_px;
flat in vec4 v_aux;
flat in vec4 v_bounds;
flat in vec4 v_params;
out vec4 o;
void main() {
  if (u_mode == 1) { o = vec4(1.0); return; }
  if (u_mode == 0) {
    float sigma = max(v_params.x, 1e-3);
    float dx = min(v_px.x - v_aux.x, v_aux.x + v_aux.z - v_px.x);
    float dy = min(v_px.y - v_aux.y, v_aux.y + v_aux.w - v_px.y);
    float a = smoothstep(-sigma, sigma, dx) * smoothstep(-sigma, sigma, dy) * v_params.y;
    o = vec4(0.0, 0.0, 0.0, a);
    return;
  }
  vec2 uv = clamp(v_uv, v_bounds.xy, v_bounds.zw);
  vec4 c = texture(u_tile, uv);
  o = vec4(c.rgb * c.a, c.a);
}`;

const STRIDE = 16;

interface Entry { tex: WebGLTexture; w: number; h: number }

export class WebGl2Compositor implements Compositor {
  readonly name = 'WebGL2';
  onLost: ((why: string) => void) | null = null;
  canvas: HTMLCanvasElement | null = null;
  private gl: WebGL2RenderingContext | null = null;
  private prog: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private vbo: WebGLBuffer | null = null;
  private cap = 0;
  private samplers: { linear: WebGLSampler; nearest: WebGLSampler } | null = null;
  private uSize: WebGLUniformLocation | null = null;
  private uMode: WebGLUniformLocation | null = null;
  private readonly tiles = new Map<string, Entry>();
  private dead = false;
  private readonly onCtxLost = (e: Event): void => { e.preventDefault(); this.lose('webglcontextlost'); };

  async init(canvas: HTMLCanvasElement): Promise<boolean> {
    try {
      const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, premultipliedAlpha: false, preserveDrawingBuffer: false, powerPreference: 'high-performance' }) as WebGL2RenderingContext | null;
      if (!gl) return false;
      const compile = (type: number, src: string): WebGLShader => {
        const s = gl.createShader(type)!;
        gl.shaderSource(s, src);
        gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'shader');
        return s;
      };
      const prog = gl.createProgram()!;
      gl.attachShader(prog, compile(gl.VERTEX_SHADER, VS));
      gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, FS));
      gl.linkProgram(prog);
      if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog) ?? 'link');
      this.prog = prog;
      this.uSize = gl.getUniformLocation(prog, 'u_size');
      this.uMode = gl.getUniformLocation(prog, 'u_mode');
      gl.useProgram(prog);
      gl.uniform1i(gl.getUniformLocation(prog, 'u_tile'), 0);
      this.vao = gl.createVertexArray();
      this.vbo = gl.createBuffer();
      gl.bindVertexArray(this.vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.vbo);
      for (let i = 0; i < 4; i++) { gl.enableVertexAttribArray(i); gl.vertexAttribDivisor(i, 1); }
      const smp = (f: number): WebGLSampler => {
        const s = gl.createSampler()!;
        gl.samplerParameteri(s, gl.TEXTURE_MIN_FILTER, f);
        gl.samplerParameteri(s, gl.TEXTURE_MAG_FILTER, f);
        gl.samplerParameteri(s, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.samplerParameteri(s, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        return s;
      };
      this.samplers = { linear: smp(gl.LINEAR), nearest: smp(gl.NEAREST) };
      gl.enable(gl.BLEND);
      gl.blendFuncSeparate(gl.ONE, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      gl.disable(gl.DEPTH_TEST);
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
      this.gl = gl;
      this.canvas = canvas;
      canvas.addEventListener('webglcontextlost', this.onCtxLost);
      return true;
    } catch {
      return false;
    }
  }

  private lose(why: string): void {
    if (this.dead) return;
    this.dead = true;
    this.onLost?.(why);
  }

  upload(key: string, source: TileSource): void {
    const gl = this.gl;
    if (!gl || this.dead || gl.isContextLost()) return;
    this.evict(key);
    const w = source.width, h = source.height;
    if (w < 1 || h < 1) return;
    const tex = gl.createTexture();
    if (!tex) return;
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, source as ImageBitmap | ImageData);
    this.tiles.set(key, { tex, w, h });
  }

  has(key: string): boolean { return this.tiles.has(key); }

  evict(key: string): void {
    const t = this.tiles.get(key);
    if (!t) return;
    this.tiles.delete(key);
    try { this.gl?.deleteTexture(t.tex); } catch { }
  }

  draw(frame: Frame): string[] {
    const gl = this.gl;
    if (!gl || this.dead || gl.isContextLost() || !this.canvas) return [];
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

    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(frame.bg[0] / 255, frame.bg[1] / 255, frame.bg[2] / 255, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (all.byteLength) {
      gl.useProgram(this.prog);
      gl.bindVertexArray(this.vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.vbo);
      if (all.byteLength > this.cap) {
        this.cap = Math.max(4096, 1 << Math.ceil(Math.log2(all.byteLength)));
        gl.bufferData(gl.ARRAY_BUFFER, this.cap, gl.DYNAMIC_DRAW);
      }
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, all);
      gl.uniform2f(this.uSize, frame.width, frame.height);
      const point = (index: number): void => {
        for (let i = 0; i < 4; i++) gl.vertexAttribPointer(i, 4, gl.FLOAT, false, STRIDE * 4, index * STRIDE * 4 + i * 16);
      };
      if (ns) { gl.uniform1i(this.uMode, 0); point(0); gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, ns); }
      if (np) { gl.uniform1i(this.uMode, 1); point(ns); gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, np); }
      if (drawn.length) {
        gl.uniform1i(this.uMode, 2);
        gl.activeTexture(gl.TEXTURE0);
        drawn.forEach((d, i) => {
          gl.bindTexture(gl.TEXTURE_2D, d.e.tex);
          gl.bindSampler(0, d.nearest ? this.samplers!.nearest : this.samplers!.linear);
          point(ns + np + i);
          gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, 1);
        });
      }
    }
    return missing;
  }

  resize(width: number, height: number, dpr: number): void {
    if (this.canvas) sizeCanvas(this.canvas, width, height, dpr);
  }

  destroy(): void {
    this.dead = true;
    this.canvas?.removeEventListener('webglcontextlost', this.onCtxLost);
    const gl = this.gl;
    if (gl && !gl.isContextLost()) {
      for (const t of this.tiles.values()) gl.deleteTexture(t.tex);
      gl.getExtension('WEBGL_lose_context')?.loseContext();
    }
    this.tiles.clear();
    this.gl = null;
  }
}
