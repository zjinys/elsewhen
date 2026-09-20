"""Final cleanup: icon = gentle pass from original; lockup = high-sensitivity watermark fill."""
from PIL import Image

def brightness(p):
    return (p[0] + p[1] + p[2]) / 3

def inpaint(px, w, h, mask):
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

def dilate(mask, w, h, d):
    out = set()
    for (x, y) in mask:
        for dy in range(-d, d + 1):
            for dx in range(-d, d + 1):
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and 0 <= ny < h:
                    out.add((nx, ny))
    return out

# --- icon: gentle pass1 from pristine original ---
im = Image.open('Minimalist_mobile_app_icon_for_2026-09-18T03-42-13.png').convert('RGB')
w, h = im.size
px = im.load()
mask = set()
for y in range(1280, h):
    for x in range(1100, w):
        if brightness(px[x, y]) > 150:
            if any(0 <= x+dx < w and 0 <= y+dy < h and brightness(px[x+dx, y+dy]) < 80
                   for dy in (-12, 0, 12) for dx in (-12, 0, 12)):
                mask.add((x, y))
mask = dilate(mask, w, h, 3)
print('icon masked:', len(mask))
inpaint(px, w, h, mask)
im.save('elsewhen-concept-icon.png')
print('saved elsewhen-concept-icon.png')

# --- lockup: faint gray watermark on white ---
im = Image.open('Minimalist_logo_lockup_for__El_2026-09-18T03-42-10.png').convert('RGB')
w, h = im.size
px = im.load()
mask = set()
for y in range(860, h):
    for x in range(1100, w):
        if brightness(px[x, y]) < 246:
            mask.add((x, y))
mask = dilate(mask, w, h, 3)
print('lockup masked:', len(mask))
inpaint(px, w, h, mask)
im.save('elsewhen-concept-lockup.png')
print('saved elsewhen-concept-lockup.png')
