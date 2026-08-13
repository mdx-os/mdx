# UI Visual Identity

MDx should feel calm, capable, and inspectable. The interface can be visually strong without turning system internals into the product story.

## Typography

- Body: DM Sans
- Display: Satoshi
- Mono: JetBrains Mono

Fonts are self-hosted. Product surfaces must not depend on remote font CSS or a font CDN.

## Tokens

`generated/ui/mdx-theme.css` is the production source of truth for color, spacing, radii, motion, and typography tokens. Promote shared visual decisions into tokens instead of copying values between apps.

## Product accents

- Twin: blue
- Forge: green or blue, depending on the work context
- Message: cyan
- Pages: amber

Accents are recognition cues, not decoration. They should help someone understand where they are and what kind of work is happening.

## Product bar

Interfaces should be dense enough to be useful and quiet enough to read. Prefer a strong first viewport, plain human language, visible next actions, and progressive disclosure for receipts or low-level evidence. Validate significant visual changes in light and dark modes.
