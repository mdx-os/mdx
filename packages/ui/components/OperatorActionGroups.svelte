<script>
  import { humanAction } from "../humanCopy.js";
  import OperatorCard from "./OperatorCard.svelte";

  export let safeActions = [];
  export let blockedActions = [];
  export let safeTitle = "inspect-only actions";
  export let blockedTitle = "actions closed";
  export let productTitle = "Operator workspace";
  export let productBody = "The shell follows the contract before becoming a fuller product surface.";
  export let productMeta = "Proof stays inspectable. It does not lead the story.";

  $: safePreview = safeActions.slice(0, 5).map((item) => String(item.label ?? item.id ?? item).toLowerCase());
  $: blockedPreview = blockedActions.slice(0, 5).map(humanAction);
</script>

<div class="action-groups" data-ui-operator-action-groups>
  <OperatorCard
    tone="success"
    eyebrow="Safe controls"
    title={`${safeActions.length} ${safeTitle}`}
    body={safePreview.join(", ")}
    meta="Navigation and local view state only."
  />
  <OperatorCard
    tone="warning"
    eyebrow="Blocked controls"
    title={`${blockedActions.length} ${blockedTitle}`}
    body={blockedPreview.join(", ")}
    meta="No hidden work, no shadow state, no unproved authority path."
  />
  <OperatorCard eyebrow="Product posture" title={productTitle} body={productBody} meta={productMeta} />
</div>

<style>
  .action-groups {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 940px) {
    .action-groups {
      grid-template-columns: 1fr 1fr;
    }
  }

  @media (max-width: 720px) {
    .action-groups {
      grid-template-columns: 1fr;
    }
  }
</style>
