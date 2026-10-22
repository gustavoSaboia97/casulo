# Vue 3 and TypeScript

Follow the project's existing conventions first; these are the defaults.

## Conventions

- Single-file components with `<script setup lang="ts">` and the Composition
  API. One component per file, named in `PascalCase` (`ProjectList.vue`).
- Props and emits typed with `defineProps<...>()` and `defineEmits<...>()`.
  Props go down, events go up; never change a prop.
- Logic outside templates: data fetching and state in composables
  (`useProjects.ts`), pure rules in plain functions. Components display and
  forward events.
- Shared state in a store (Pinia) only when two distant components need it;
  otherwise props and composables.
- Styles `scoped`, using only design tokens (`var(--space-4)`).
- Never `v-html` with data from users, APIs or URLs. If HTML must be shown,
  sanitize it with a library the project already uses, and say so in review.

## Composable: state with every case

```ts
// useProjects.ts
import { ref } from "vue";
import { fetchProjects, type Project } from "@/services/projects";

export function useProjects() {
  const projects = ref<Project[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      projects.value = await fetchProjects();
    } catch {
      error.value = "Could not load projects. Check your connection and try again.";
    } finally {
      loading.value = false;
    }
  }

  return { projects, loading, error, load };
}
```

## Component: semantic HTML and all states

```vue
<!-- ProjectList.vue -->
<script setup lang="ts">
import { onMounted } from "vue";
import { useProjects } from "./useProjects";

const { projects, loading, error, load } = useProjects();
onMounted(load);
</script>

<template>
  <section aria-labelledby="projects-title">
    <h2 id="projects-title">Projects</h2>

    <p v-if="loading" role="status">Loading projects…</p>

    <div v-else-if="error" role="alert">
      <p>{{ error }}</p>
      <button type="button" @click="load">Try again</button>
    </div>

    <p v-else-if="projects.length === 0">
      No projects yet. <a href="/projects/new">Create your first project</a>.
    </p>

    <ul v-else>
      <li v-for="project in projects" :key="project.id">{{ project.name }}</li>
    </ul>
  </section>
</template>

<style scoped>
section { display: grid; gap: var(--space-4); }
</style>
```

## Test: what the user sees (vitest and @testing-library/vue)

Find by role, label and text; never by class or component internals. Three
blocks, as in the `tdd` skill.

```ts
import { render, screen } from "@testing-library/vue";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import ProjectList from "./ProjectList.vue";
import * as api from "@/services/projects";

it("offers a retry when projects fail to load", async () => {
  // Given
  vi.spyOn(api, "fetchProjects")
    .mockRejectedValueOnce(new Error("offline"))
    .mockResolvedValueOnce([{ id: 1, name: "casulo" }]);
  render(ProjectList);

  // When
  await userEvent.click(await screen.findByRole("button", { name: "Try again" }));

  // Then
  expect(await screen.findByText("casulo")).toBeTruthy();
});
```

## Smells

- A component over about 200 lines, or a template with logic in expressions:
  split it or move logic to a composable.
- Props passed through three or more levels unchanged: use provide/inject or
  a store.
- `watch` that only copies one value into another: use `computed`.
- A test that needs `wrapper.vm` or a CSS selector: it tests internals.
