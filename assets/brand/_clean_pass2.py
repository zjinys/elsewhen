"""Second cleanup pass on concept-icon residuals + crop lockup corner for check."""
from PIL import Image

# --- pass 2 on icon: lower threshold, wider region ---
src = 'elsewhen-concept-icon.png'
im = Image.open(src).convert('RGB')
w, h = im.size
px = im.load()

def brightness(p):
    return (p[0] + p[1] + p[2]) / 3

x0, y0 = 1050, 1230
mask = set()
for y in range(y0, h):
    for x in range(x0, w):
        b = brightness(px[x, y])
        if b > 110:
            near_dark = any(
                0 <= x+dx < w and 0 <= y+dy < h and brightness(px[x+dx, y+dy]) < 90
                for dy in (-14, -7, 0, 7, 14) for dx in (-14, -7, 0, 7, 14)
            )
            if near_dark:
                mask.add((x, y))
# dilate 4
dil = set()
for (x, y) in mask:
    for dy in range(-4, 5):
        for dx in range(-4, 5):
            nx, ny = x+dx, y+dy
            if 0 <= nx < w and 0 <= ny < h:
                dil.add((nx, ny))
print('pass2 masked:', len(dil))

for (x, y) in dil:
    for r in range(4, 70, 4):
        samples = []
        for dy in range(-r, r+1, 2):
            for dx in range(-r, r+1, 2):
                nx, ny = x+dx, y+dy
                if 0 <= nx < w and 0 <= ny < h and (nx, ny) not in dil:
                    samples.append(px[nx, ny])
        if len(samples) >= 6:
            rs = sorted(s[0] for s in samples); gs = sorted(s[1] for s in samples); bs = sorted(s[2] for s in samples)
            m = len(samples)//2
            px[x, y] = (rs[m], gs[m], bs[m])
            break
im.save(src)
print('saved', src)

# --- crop lockup bottom-right for visual check ---
lk = Image.open('elsewhen-concept-lockup.png').convert('RGB')
lw, lh = lk.size
lk.crop((lw-700, lh-350, lw, lh)).save('_lockup_corner_check.png')
print('corner crop saved')
