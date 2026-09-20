"""Remove corner watermarks from generated logo renders via local inpainting."""
from PIL import Image

# file, search region (x0, y0), mode: 'bright_on_dark' | 'dark_on_light'
JOBS = [
    ('Minimalist_mobile_app_icon_for_2026-09-18T03-42-13.png',
     'elsewhen-concept-icon.png', (1100, 1280), 'bright_on_dark_with_white_margin'),
    ('Minimalist_logo_lockup_for__El_2026-09-18T03-42-10.png',
     'elsewhen-concept-lockup.png', (1100, 880), 'dark_on_light'),
    ('Elegant_brand_emblem_for__Else_2026-09-18T03-42-18.png',
     'elsewhen-concept-emblem.png', (1000, 1260), 'bright_on_dark'),
]
DILATE = 3


def brightness(p):
    return (p[0] + p[1] + p[2]) / 3


def build_mask(px, w, h, x0, y0, mode):
    mask = set()
    for y in range(max(0, y0), h):
        for x in range(max(0, x0), w):
            b = brightness(px[x, y])
            if mode == 'dark_on_light':
                if b < 225:
                    mask.add((x, y))
            elif mode == 'bright_on_dark':
                if b > 60:
                    mask.add((x, y))
            else:  # bright watermark on dark square, plus white margin around it
                if b > 150:
                    # only flag if near a dark pixel (i.e. sitting on the dark square)
                    near_dark = False
                    for dy in (-12, 0, 12):
                        for dx in (-12, 0, 12):
                            nx, ny = x + dx, y + dy
                            if 0 <= nx < w and 0 <= ny < h and brightness(px[nx, ny]) < 80:
                                near_dark = True
                                break
                        if near_dark:
                            break
                    if near_dark:
                        mask.add((x, y))
    # dilate
    out = set()
    for (x, y) in mask:
        for dy in range(-DILATE, DILATE + 1):
            for dx in range(-DILATE, DILATE + 1):
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and 0 <= ny < h:
                    out.add((nx, ny))
    return out


def inpaint(px, w, h, mask):
    """Fill masked pixels with median of nearest unmasked pixels (expanding window)."""
    for (x, y) in mask:
        for r in range(3, 60, 3):
            samples = []
            for dy in range(-r, r + 1, 2):
                for dx in range(-r, r + 1, 2):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in mask:
                        samples.append(px[nx, ny])
            if len(samples) >= 6:
                rs = sorted(s[0] for s in samples)
                gs = sorted(s[1] for s in samples)
                bs = sorted(s[2] for s in samples)
                m = len(samples) // 2
                px[x, y] = (rs[m], gs[m], bs[m], 255)
                break


for src, dst, (x0, y0), mode in JOBS:
    im = Image.open(src).convert('RGB')
    w, h = im.size
    px = im.load()
    mask = build_mask(px, w, h, x0, y0, mode)
    print(f'{src}: masked {len(mask)} px in region x>={x0}, y>={y0}')
    if mask:
        inpaint(px, w, h, mask)
    im.save(dst)
    print(f'  -> saved {dst}')
