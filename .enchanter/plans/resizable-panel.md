# Resizable Panel Window

## Goal

Make `imsai-panel` fit on a 1600x900 laptop screen. Default size
**1024x680**, resizable down to a sane floor. Single window, layout
scales with size. The whole panel + CRT + I/O log remain visible at all
sizes >= the minimum.

## Approach: render-to-texture

Render the entire UI once per frame into a 1024x680 `RenderTexture2D`,
then blit that texture to the window with `draw_texture_pro` scaled to
fit. The layout code (constants, coordinates, fonts) is **completely
unchanged**; the texture is the source of truth and the window is just
a viewport onto it.

```
target = RenderTexture2D(1024, 680)        // created once
texture_filter = TEXTURE_FILTER_BILINEAR  // smooth scaling

each frame:
    scale = min(win_w / 1024, win_h / 680).max(0.65)
    dst_w = (1024 * scale).round()
    dst_h = (680 * scale).round()
    rl.draw_texture_mode(thread, &mut target, |t| {
        // existing draw code, layout-space coords, t.clear_background(bg), etc.
    });
    d.clear_background(bg);    // window background behind scaled blit
    d.draw_texture_pro(
        &target,
        Rectangle { x: 0, y: 0, width: 1024, height: -680 }, // src (y flipped for RT)
        Rectangle { x: (win_w - dst_w)/2, y: (win_h - dst_h)/2,
                   width: dst_w, height: dst_h },
        Vector2::new(0, 0),
        0.0,
        Color::WHITE,
    );
```

Render textures in raylib are flipped vertically (the C convention), so
the source rect has negative height and the texture filter is bilinear
to keep the scaled output smooth.

The blit is centered in the window with letterboxing if the aspect
ratios don't match. The `MIN_SCALE` clamp on the uniform scale factor
ensures the blit never gets so small things become illegible.

### Mouse coordinate conversion

`get_mouse_position` returns window pixels. Hit tests need layout
coordinates (1024x680). With the centered blit:

```
sx = REF_W * scale  // rendered blit width in window pixels
sy = REF_H * scale
origin_x = (win_w - sx) / 2
origin_y = (win_h - sy) / 2
layout_x = (mouse.x - origin_x) / scale
layout_y = (mouse.y - origin_y) / scale
```

When `scale == 1.0` and the window is exactly REF_W x REF_H, the
origin is (0,0) and `layout = mouse / 1.0`. When the window is larger
than the blit, the mouse lands in the letterbox area and `layout` goes
out of range; the hit tests already guard against that with their
existing bounds checks.

## Constants

| Name        | Value      | Meaning                                          |
|-------------|------------|--------------------------------------------------|
| `REF_W`     | 1024       | Render texture width; also the scale-1.0 window width |
| `REF_H`     | 680        | Render texture height                             |
| `MIN_SCALE` | 0.65       | Smallest allowed blit scale                       |
| `MIN_W`     | 832        | ceil(1024 * 0.65); raylib's window-min-size floor |
| `MIN_H`     | 442        | ceil(680 * 0.65)                                  |

`W` and `H` constants (1280, 840) are deleted. The few code paths that
used them are redirected to `REF_W`/`REF_H` since the layout IS the
1024x680 reference space now.

## Raylib wiring

```rust
let (mut rl, thread) = raylib::init()
    .size(REF_W, REF_H)
    .resizable()
    .title("IMSAI 8080 Microcomputer")
    .build();

rl.set_window_min_size(MIN_W, MIN_H);
rl.set_target_fps(30);

let mut target = rl.load_render_texture(&thread, REF_W, REF_H);
target.texture().set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
```

## Mouse helper

```rust
let layout_mouse = |mx, my| -> Vector2 {
    let sx = REF_W as f32 * scale;
    let sy = REF_H as f32 * scale;
    let ox = (win_w as f32 - sx) * 0.5;
    let oy = (win_h as f32 - sy) * 0.5;
    Vector2::new((mx - ox) / scale, (my - oy) / scale)
};
```

All three existing `get_mouse_position()` calls switch to this
helper. (Three sites: switch clicks, picker clicks, no third — verify.)

## Files touched

- `src/bin/panel.rs` only.

## Testing

- `cargo build --release` must succeed.
- `cargo test` must still pass.
- Visual: render with `--shot panel_default.png` and check the screenshot
  is the 1024x680 reference (not 1280x840).
- Manual: resize the window smaller and verify panel/CRT/paddles all
  scale together; verify clicks still hit the right switches.
