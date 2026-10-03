# Arc UI adaptations

Source: https://github.com/kuratlielia/arc-library (MIT). Registry retrieved on 3 October 2026.

Honeycomb uses SolidJS. `Arc.tsx` and `arc.css` adapt the free badge and segmented-control source to Solid native elements, retaining semantic colors, roving keyboard selection, overflow scrolling and reduced motion. The shared foundation is copied from the free registry; its global focus suppression is removed so Honeycomb retains visible keyboard focus. Buttons and application cards adapt the free button/card CSS patterns in `honeycomb-theme.css`. No React runtime or Pro component is included.

Brand overrides use IAM blue, IBM Plex Sans, and smaller radii. Card navigation keeps existing application routing rather than Arc quick-look overlays. Motion uses the foundation easing and durations with CSS; no spring dependency is needed.

## Official documentation

- https://uiarc.dev/components/badge/markdown
- https://uiarc.dev/components/segmented-control/markdown
- https://uiarc.dev/components/button/markdown
- https://uiarc.dev/components/card/markdown

## Registry SHA256

- `https://uiarc.dev/r/arc-foundation.json`: `d2c069638ede51252811a38e62170fc6db92bc8bb8248bdc04b8b36ebd318bcc`
- `https://uiarc.dev/r/button.json`: `6a3cd1af1fc2842dbd11917aa84594fbc6a46c616d925882e772768b04ed9a90`
- `https://uiarc.dev/r/badge.json`: `1c04487786c844d95fdf3453a7d60a1e053d00265b4062b2dd1c5513f5e1cd3a`
- `https://uiarc.dev/r/card.json`: `ca5700b79ed9948ce3397a9ff6c853f5594286770a0b4ea49c23166306df90e2`
- `https://uiarc.dev/r/segmented-control.json`: `cff66d900ec2cdd9da706415421ac82849f28390d0a9e305e2973c5f6d8fcfa4`
