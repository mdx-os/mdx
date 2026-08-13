# UI Product Surface Contract

This contract keeps Twin, Forge, Pages, Message, and supporting surfaces feeling like one product.

## Page structure

- The page heading matches the navigation label.
- The first viewport makes the current object or state understandable.
- One primary action advances the main job.
- Empty states explain what can be created or connected next.
- Secondary evidence stays behind a disclosure, inspector, or details view.

## Copy

- Lead with what a person can understand or decide.
- Prefer concrete nouns and verbs over platform jargon.
- Do not use route names, receipt IDs, session IDs, or internal tokens as primary labels.
- Say when a capability is local-only, simulated, unavailable, or waiting for configuration.
- Do not claim external readiness without observed evidence.

## Interaction

- Consequential actions show scope and require the appropriate approval.
- Errors preserve the person's work and provide a safe next step.
- Loading and unavailable states should not resemble success.
- Keyboard, focus, contrast, and reduced-motion behavior are part of the feature.

## Visual system

Use the shared tokens in `generated/ui/mdx-theme.css`. Product accents help with recognition but should not fragment layout or component behavior. New patterns should earn their way into shared components instead of being copied between pages.

## Validation

Build and smoke test the host after product-surface changes:

```sh
make web-build
make web-smoke
```

Significant visual changes should also be reviewed in light and dark modes at representative desktop and mobile widths.
