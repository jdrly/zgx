struct Vertex { @location(0) rect:vec4<f32>, @location(1) uv:vec4<f32>, @location(2) color:vec4<f32>, @location(3) fade:vec4<f32>, @location(4) options:vec4<f32>, @location(5) shape:vec4<f32>, @location(6) border:vec4<f32>, @location(7) mask:vec4<f32> }
// Only `uv` and `point` vary across a quad; the rest are per-quad constants.
// Flat, they cost nothing per pixel (a software rasterizer interpolates every
// smooth varying for every pixel it shades).
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) point:vec2<f32>, @location(3) @interpolate(flat) fade:vec4<f32>, @location(4) @interpolate(flat) options:vec4<f32>, @location(5) @interpolate(flat) shape:vec4<f32>, @location(6) @interpolate(flat) border:vec4<f32>, @location(7) @interpolate(flat) mask:vec4<f32> }
// Group 1: one immutable buffer per viewport size, so layers never rewrite it.
@group(1) @binding(0) var<uniform> viewport:vec4<f32>;
@group(0) @binding(1) var atlas:texture_2d<f32>;
// A clip region quads share (`mask.x`: its index plus one, 0 for none): up
// to two rounded boxes (x, y, width, height; a negative width for none)
// with their radii (top-left, top-right, bottom-right, bottom-left), cut
// with antialiased corners; and a box (x0, y0, x1, y1) whose content fades
// out over bands at its edges (top, right, bottom, left), quadratically, as
// Zeron fades transcript edges. Logical units of the target, where the
// pixel is on screen (a transformed quad's `point` is its own).
// `color0..2` (`tint.x` on): its colour filters as rows of an affine map
// of straight RGB (brightness, contrast, saturate), applied as the colour
// is written. `tint.y`, `tint.z`: the rounded boxes' feathers, how far
// inside their edges their content fades out from (0: cut at the edge).
struct Region { round0:vec4<f32>, radii0:vec4<f32>, round1:vec4<f32>, radii1:vec4<f32>, fade_box:vec4<f32>, fade:vec4<f32>, color0:vec4<f32>, color1:vec4<f32>, color2:vec4<f32>, tint:vec4<f32> }
@group(1) @binding(1) var<storage, read> regions:array<Region>;
fn rounded(b:vec4<f32>, r:vec4<f32>, feather:f32, p:vec2<f32>)->f32 {
 if b.z<0. {return 1.;}
 let half=b.zw*.5; let q=p-b.xy-half;
 let radius=select(select(r.w,r.z,q.x>0.),select(r.x,r.y,q.x>0.),q.y<0.);
 let d=abs(q)-half+vec2(radius); let sdf=length(max(d,vec2(0.)))+min(max(d.x,d.y),0.)-radius;
 if feather>0. {let t=clamp(-sdf/feather,0.,1.); return t*t*(3.-2.*t);}
 return clamp(.5-sdf*viewport.z,0.,1.);
}
// `premultiplied` through region `index`'s colour filters, if any.
fn tinted(index:f32, premultiplied:vec4<f32>)->vec4<f32> {
 if index<.5 {return premultiplied;}
 let g=regions[u32(index)-1u];
 if g.tint.x<.5 || premultiplied.a<=0. {return premultiplied;}
 let c=vec4(premultiplied.rgb/premultiplied.a,1.);
 let rgb=clamp(vec3(dot(g.color0,c),dot(g.color1,c),dot(g.color2,c)),vec3(0.),vec3(1.));
 return vec4(rgb*premultiplied.a,premultiplied.a);
}
fn region(index:f32, position:vec2<f32>)->f32 {
 let g=regions[u32(index)-1u]; let p=position/viewport.z;
 var k=rounded(g.round0,g.radii0,g.tint.y,p)*rounded(g.round1,g.radii1,g.tint.z,p);
 if g.fade.x>0. {let t=clamp((p.y-g.fade_box.y)/g.fade.x,0.,1.); k*=t*t;}
 if g.fade.y>0. {let t=clamp((g.fade_box.z-p.x)/g.fade.y,0.,1.); k*=t*t;}
 if g.fade.z>0. {let t=clamp((g.fade_box.w-p.y)/g.fade.z,0.,1.); k*=t*t;}
 if g.fade.w>0. {let t=clamp((p.x-g.fade_box.x)/g.fade.w,0.,1.); k*=t*t;}
 return k;
}
@group(0) @binding(2) var tex_sampler:sampler;
// `ImageData::characters`' font: glyph `i` of ` .-+xXS80#@` (darkest first),
// 5 by 7 pixels, rows top first, five bits a row (the leftmost highest): rows
// 0 to 5 in `x`, row 6 in `y`. A switch, not an array a runtime index reads
// (which Metal spills to memory).
fn glyph(i:u32)->vec2<u32> {
 switch i {
  case 1u: {return vec2(402653184u,12u);} case 2u: {return vec2(1015808u,0u);}
  case 3u: {return vec2(139432064u,0u);} case 4u: {return vec2(340083712u,17u);}
  case 5u: {return vec2(581052977u,17u);} case 6u: {return vec2(35078671u,30u);}
  case 7u: {return vec2(588727854u,14u);} case 8u: {return vec2(597347886u,14u);}
  case 9u: {return vec2(368409930u,10u);} case 10u: {return vec2(561700398u,15u);}
  default: {return vec2(0u);}
 }
}
// A cell's own random number, 0 to 1 (a 32-bit integer hash).
fn cell_hash(c:vec2<u32>)->f32 {
 var h=c.x*1664525u+c.y*1013904223u+0x9e3779b9u;
 h^=h>>16u; h*=2246822519u; h^=h>>13u; h*=3266489917u; h^=h>>16u;
 return f32(h&16777215u)/16777216.;
}
@vertex fn vs(v:Vertex,@builtin(vertex_index) i:u32)->Out {
 // A quad is a four-vertex strip, (0,0) (1,0) (0,1) (1,1): its two
 // triangles with two fewer vertex invocations than a list. Corners come
 // from the index's bits: a runtime index into a local array makes Metal
 // reserve per-thread scratch memory for every thread the GPU can run.
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let p=v.rect.xy+corner*v.rect.zw;
 var position=p;
 if v.options.z>0.5 {position=vec2(dot(v.shape.xy,p)+v.border.x,dot(v.shape.zw,p)+v.border.y);}
 var o:Out; o.position=vec4(position.x/viewport.x*2.-1.,1.-position.y/viewport.y*2.,0.,1.);o.uv=v.uv.xy+corner*v.uv.zw;o.color=v.color;o.point=p;o.fade=v.fade;o.options=v.options;o.shape=v.shape;o.border=v.border;o.mask=v.mask;return o;
}
fn shade(input:Out)->vec4<f32> {
 // Where the pixel is, in logical units: from its own position, so a quad
 // drawn in pieces (`push_panel`) shades every pixel as the whole would. A
 // transformed quad keeps its interpolated, untransformed position.
 var v=input;
 if v.options.z<0.5 {v.point=v.position.xy/viewport.z;}
 var color=v.color;
 if v.options.z<0.5 && (v.shape.w>0.5 || v.shape.z>0.) {
   var corner=v.shape.x;
   if v.options.w>0.5 {
     let right=v.point.x>v.fade.x+v.fade.z*0.5; let bottom=v.point.y>v.fade.y+v.fade.w*0.5;
     corner=select(select(v.border.x,v.border.y,right),select(v.border.w,v.border.z,right),bottom);
   }
   let radius=clamp(corner,0.,min(v.fade.z,v.fade.w)*0.5);
   let q=abs(v.point-v.fade.xy-v.fade.zw*0.5)-(v.fade.zw*0.5-vec2(radius));
   let distance=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;
   if v.shape.y<0. {
     // A ring (a border alone, `ring_quad`): side widths (top, right,
     // bottom, left) packed two to a lane in 16ths of a pixel; inside, the
     // box inset by them, its corners as much rounder as they are thinner.
     let a=u32(v.shape.x); let b=u32(v.shape.z);
     let sides=vec4(f32(a&4095u),f32(a>>12u),f32(b&4095u),f32(b>>12u))/16.;
     let right=v.point.x>v.fade.x+v.fade.z*0.5; let bottom=v.point.y>v.fade.y+v.fade.w*0.5;
     let across=select(sides.w,sides.y,right); let down=select(sides.x,sides.z,bottom);
     let box=vec4(v.fade.xy+sides.wx,max(v.fade.zw-sides.wx-sides.yz,vec2(0.)));
     let inner_radius=clamp(radius-(across+down)*0.5,0.,min(box.z,box.w)*0.5);
     let iq=abs(v.point-box.xy-box.zw*0.5)-(box.zw*0.5-vec2(inner_radius));
     let inside=length(max(iq,vec2(0.)))+min(max(iq.x,iq.y),0.)-inner_radius;
     // Fine derivatives (each pixel's own): Direct3D's `fwidth` is coarse,
     // one per 2×2 block, and softened rounded rings (a pixel 1.4 px inside
     // a 4 px border came out 94% covered). Metal's and Vulkan's were fine.
     let aa=max(fwidthFine(distance),0.5);
     let hollow=select(0.,clamp(0.5-inside/aa,0.,1.),box.z>0.&&box.w>0.);
     color.a*=max(clamp(0.5-distance/aa,0.,1.)-hollow,0.);
   } else if v.shape.z>0. {
     // A box shadow (`shape.z` its sigma, half the CSS blur): the box
     // blurred covers 0.5·erfc(d/(sigma·√2)) at signed distance d, half at
     // its edge (not full: that read as a hard, dark band).
     let x=distance/(v.shape.z*1.41421356);
     let x2=x*x;
     let e=sign(x)*sqrt(1.-exp(-x2*(1.27323954+0.147*x2)/(1.+0.147*x2)));
     color.a*=0.5-0.5*e;
   }
   // A square box's corner on whole pixels is covered as its edges are:
   // `fwidth` (the slopes' sum) adds both axes' there when the pixel's 2×2
   // block reaches outside (a corner at an odd pixel), and shaded 3/4 what
   // the basic shader draws whole, so a box's pixels hung on its batch.
   else {let dx=abs(dpdxFine(distance));let dy=abs(dpdyFine(distance));let aa=max(select(dx+dy,max(dx,dy),radius<=0.),0.5);let outer=clamp(0.5-distance/aa,0.,1.);let inner=clamp(0.5-(distance+v.shape.y)/aa,0.,1.);let alpha=color.a*inner+v.border.a*(outer-inner);let rgb=select(vec3(0.),(color.rgb*color.a*inner+v.border.rgb*v.border.a*(outer-inner))/max(alpha,0.00001),alpha>0.);color=vec4(rgb,alpha);}
 }

 // Premultiplied texels carry their alpha already: the colour stays
 // premultiplied from here (`basic`'s product), scaled by whatever later
 // scales alpha, rather than divided by it and multiplied back.
 var premultiplied=false; var rgb=vec3(0.); var scale=1.;
 if v.options.y>0.5 && v.options.w<-.5 {
   // Characters (`options.w` -1, where positive is per-corner radii;
   // `options.y` 2 plus a font pixel's size in
   // points): cells 6 by 8 font pixels on the screen's grid, each a glyph
   // picked by the brightness at its centre and lit in that colour at full
   // value, over the image dimmed. Images keep their colour white, so its
   // lanes carry the reveal (front, band, raggedness) and `border.zw` its
   // centre's offset.
   let size=(v.options.y-2.)*viewport.z;
   let reveal=v.color.rgb; color=vec4(1.,1.,1.,color.a);
   let cell_size=vec2(6.,8.)*size;
   let cell=floor(v.position.xy/cell_size);
   let to_centre=(cell+.5)*cell_size-v.position.xy;
   let at=textureSampleLevel(atlas,tex_sampler,v.uv+dpdxFine(v.uv)*to_centre.x+dpdyFine(v.uv)*to_centre.y,0.);
   let own=textureSample(atlas,tex_sampler,v.uv);
   let c=vec2<u32>(vec2<i32>(cell));
   // The reveal, from the point it opens at out to a rounded box at 1: the
   // picture fades in smoothly, pixel by pixel, behind the band (no cells
   // popping); characters scatter ahead of it, sparse and faint at the
   // band's outer edge, denser and brighter inward, scrambling until the
   // picture reaches them.
   var shown=1.; var glyph_alpha=1.; var scrambling=false; var scramble=0u;
   if reveal.y>0. {
     let q=abs(v.position.xy/viewport.z/viewport.xy-.5-v.border.zw)*2.; let q4=q*q*q*q;
     shown=smoothstep(reveal.y*.35,reveal.y*1.35,reveal.x-sqrt(sqrt(q4.x+q4.y)));
     let qc=abs((cell+.5)*cell_size/viewport.z/viewport.xy-.5-v.border.zw)*2.; let qc4=qc*qc*qc*qc;
     let front=reveal.x-sqrt(sqrt(qc4.x+qc4.y))+(cell_hash(c)-.5)*reveal.z;
     let into=front/reveal.y;
     glyph_alpha=select(0.,smoothstep(0.,.6,into),cell_hash(c+vec2(77u,13u))<pow(clamp(into,0.,1.),.7));
     scrambling=into<1.; scramble=u32(max(into,0.)*12.);
   }
   let straight=select(vec3(0.),at.rgb/at.a,at.a>0.);
   // Glyphs for highlights, light ones for midtones, none for shadows.
   var index=u32(pow(clamp((dot(straight,vec3(.299,.587,.114))*at.a-.3)/.65,0.,1.),1.6)*10.999);
   if scrambling {index=1u+u32(cell_hash(c+vec2(scramble*7u+1u,scramble))*9.999);}
   let g=glyph(index);
   let fp=vec2<u32>(floor((v.position.xy-cell*cell_size)/size));
   var lit=0.;
   if fp.x<5u && fp.y<7u {lit=f32((select(g.y,g.x>>(fp.y*5u),fp.y<6u)>>(4u-fp.x))&1u);}
   let ink=straight/max(max(straight.r,max(straight.g,straight.b)),.25);
   // The glyph over the picture (dimmed), its strength easing from whole,
   // alone ahead of the picture, to 0.65 over it.
   let k=lit*glyph_alpha*mix(1.,.65,shown);
   let tex=vec4(ink*at.a*k+own.rgb*.88*shown*(1.-k),at.a*k+own.a*shown*(1.-k));
   premultiplied=true; rgb=tex.rgb*color.a; color.a=color.a*tex.a;
 } else if v.options.y>0.5 {
   // Dithered (`options.y` is 2 plus the dot size, in points): one sample
   // per dot, at its centre on the screen's grid (the uv there through
   // the quad's own slopes, so moved, scaled and covering images dither
   // alike), then Zeron's two levels: the colour at full value where its
   // brightest channel passes the dot's 4x4 Bayer threshold, else 8% of it.
   let dot=(v.options.y-2.)*viewport.z;
   var tex:vec4<f32>;
   if dot>0.5 {
     let cell=floor(v.position.xy/dot);
     let centre=(cell+.5)*dot-v.position.xy;
     tex=textureSampleLevel(atlas,tex_sampler,v.uv+dpdxFine(v.uv)*centre.x+dpdyFine(v.uv)*centre.y,0.);
     if tex.a>0. {
       let c=vec2<u32>(vec2<i32>(cell))&vec2(3u);
       let a=c.x^c.y; let b=c.y;
       let bayer=f32(((a&1u)<<3u)|((b&1u)<<2u)|(a&2u)|((b&2u)>>1u));
       let straight=tex.rgb/tex.a; let peak=max(straight.r,max(straight.g,straight.b));
       tex=vec4(select(straight*.08,straight/max(peak,1e-5),peak>(bayer+.5)/16.)*tex.a,tex.a);
     }
   } else {tex=textureSample(atlas,tex_sampler,v.uv);}
   if v.options.y>1.5 {premultiplied=true; rgb=color.rgb*tex.rgb*color.a; color.a=color.a*tex.a;}
   else {color=vec4(color.rgb*tex.rgb,color.a*tex.a);}
 }
 if v.options.x>0. {let distance=min(v.point-v.fade.xy,v.fade.xy+v.fade.zw-v.point); let f=clamp(min(distance.x,distance.y)/v.options.x,0.,1.); color.a*=f; scale*=f;}
 // Rounded clips and `fade_edges` around it (see `Region`).
 if v.mask.x>.5 {let k=region(v.mask.x,v.position.xy); color.a*=k; scale*=k;}
 return tinted(v.mask.x,select(vec4(color.rgb*color.a,color.a),vec4(rgb*scale,color.a),premultiplied));
}
@fragment fn fs(v:Out)->@location(0) vec4<f32> { return shade(v); }
// Single-pass presentation: the same colour to the retained target and the drawable.
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@fragment fn fs_both(v:Out)->Both { let c=shade(v); return Both(c,c); }
// Plain quads (no corners, borders, shadows, fades, regions or transforms):
// most text and backgrounds. A software rasterizer runs a fragment shader on
// the CPU for every pixel, and every branch of `shade` with it, so these use
// this one where drawing is split (see `push_draw`). Same results as `shade`.
// `texture`: 0 for a flat colour, 1 for straight-alpha samples (glyphs), 2
// for premultiplied ones (icons, images), as `options.y`.
struct Basic { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) @interpolate(flat) texture:f32 }
@vertex fn vs_basic(@location(0) rect:vec4<f32>,@location(1) uv:vec4<f32>,@location(2) color:vec4<f32>,@location(4) options:vec4<f32>,@builtin(vertex_index) i:u32)->Basic {
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let p=rect.xy+corner*rect.zw;
 var o:Basic; o.position=vec4(p.x/viewport.x*2.-1.,1.-p.y/viewport.y*2.,0.,1.);o.uv=uv.xy+corner*uv.zw;o.color=color;o.texture=options.y;return o;
}
// Straight samples: colour.rgb * tex.rgb * (colour.a * tex.a). Premultiplied
// ones already carry tex.a in tex.rgb: colour.rgb * tex.rgb * colour.a.
fn basic(v:Basic)->vec4<f32> {
 if v.texture>0.5 {let tex=textureSample(atlas,tex_sampler,v.uv); let a=v.color.a*tex.a; return vec4(v.color.rgb*tex.rgb*select(a,v.color.a,v.texture>1.5),a);}
 return vec4(v.color.rgb*v.color.a,v.color.a);
}
@fragment fn fs_basic(v:Basic)->@location(0) vec4<f32> { return basic(v); }
@fragment fn fs_basic_both(v:Basic)->Both { let c=basic(v); return Both(c,c); }
