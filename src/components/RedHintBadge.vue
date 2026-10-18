<script setup lang="ts">
/** 负库存红标（契约 7.4 / D2）：内容由父级传入（默认"负库存"）。 */
</script>

<template>
  <span
    class="inline-flex shrink-0 items-center rounded-full bg-red-100 px-2 py-0.5 text-xs font-medium text-red-700 ring-1 ring-inset ring-red-200"
  >
    <slot>负库存</slot>
  </span>
</template>
