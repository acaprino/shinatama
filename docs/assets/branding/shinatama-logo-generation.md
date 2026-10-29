# Shinatama logo: Daodan-inspired revision

## Result

- Final asset: [shinatama-logo-xenotron-glow.png](shinatama-logo-xenotron-glow.png),
  SHA-256 `8739ff0fcd1395ba764d71da5d55e79698655c8fb94cde7e9e6a97f85547c655`.
- Method: JanuraCode image edit with `gpt-image-2.5-sunburst`, 2026-10-29.
  The tool did not return a generation ID.

## References

- Original repository logo: [shinatama-logo-oni.png](shinatama-logo-oni.png),
  SHA-256 `3e1050ef2bf44acfa7c650d6255862cc7b438dc74bf7f6264dbcf2c2a5c0fe44`.
- Daodan style reference: [daodan-logo-xenotron-glow.png](https://github.com/acaprino/daodan/blob/master/docs/assets/branding/daodan-logo-xenotron-glow.png),
  SHA-256 `52e8793aea33443e52a5e8f1b27baaeada8af70502e47919bd75c2f0bdcae740e`.
  Its typography is a generated interpretation inspired by Xenotron, not a
  recorded font file.
- Actual image-edit input: [shinatama-daodan-edit-input.png](references/shinatama-daodan-edit-input.png),
  SHA-256 `095404e9865443dde0fd81268ad72454e1d07814ab208d1d10848186ca56401a`.
  It combines the original Shinatama logo with the Daodan style sample as an
  inset. The inset is not part of the intended output.
- Original in-game Shinatama portrait: [shinatama-oni-original.png](references/shinatama-oni-original.png),
  downloaded from [OniGalore](https://wiki.oni2.net/File:TXMPSHINlistening.png),
  SHA-256 `ec63d6675c392eb02251bcb602dc71ae8fdef8941a8397c06b61263bf8a65d44`.
  This third-party image is retained locally as a reference; redistribution
  rights have not been verified.

## Image-edit prompt

```text
Edit the supplied composite: the large SHINATAMA banner and character are the only artwork to revise. The smaller DAODAN wordmark in the upper-right rectangle is only a style reference. Remove that inset and its rectangular backing completely from the finished image.

Create one polished horizontal SHINATAMA logo that is visibly a companion to the DAODAN reference. Preserve the original Shinatama character on the left as closely as possible: the same anime portrait, pose, expression, pink hair, blue eyes, facial details and white-and-blue uniform. Keep the character fully clothed and recognizable; do not add another character.

Redraw the wordmark using the same angular, wide, geometric sci-fi lettering shown in the DAODAN reference, a Xenotron-inspired generated style. Match its A and N shapes, sharp clipped corners, consistent stroke proportions, cap height, baseline and spacing. Use very dark navy letter faces, thin metallic silver and ice-blue beveled outlines, and restrained blue reflections. Keep one thin silver-blue underline. The exact text must read SHINATAMA, once only: nine uppercase letters S H I N A T A M A, with all three A letters matching. No DAODAN or other text in the result.

Replace the broad saturated electric-blue glow with a much smaller, softer contact halo like the DAODAN reference: translucent violet and lilac, with faint periwinkle and muted rose accents, closely following the character and letters and gently fading away. Preserve crisp edges and readability. No broad blue cloud, blue background wash, harsh bloom, lightning, flames or rays.

Show only the finished Shinatama logo with the portrait at left and wordmark at right, on a genuinely transparent background with transparent space around the artwork and inside the letter counters. Keep generous margins. No dark panel, rectangle, painted checkerboard, mockup, watermark, caption, extra logo or extra text.
```
