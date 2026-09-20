from PIL import Image

files = {
    'A_icon': 'Minimalist_mobile_app_icon_for_2026-09-18T03-42-13.png',
    'B_lockup': 'Minimalist_logo_lockup_for__El_2026-09-18T03-42-10.png',
    'C_emblem': 'Elegant_brand_emblem_for__Else_2026-09-18T03-42-18.png',
}
for k, f in files.items():
    im = Image.open(f)
    print(k, im.size, im.mode)
    w, h = im.size
    px = im.convert('RGBA')
    for name, (x, y) in {'TR': (w-10, 10), 'BR': (w-10, h-10), 'BL': (10, h-10),
                         'center-bottom': (w//2, h-5), 'mid-right': (w-5, h//2)}.items():
        print('  ', name, px.getpixel((x, y)))
