# Code Context Engine Frontend - Architecture Documentation

## Overview

This document provides a comprehensive overview of the Code Context Engine (CCE) Frontend architecture, including component hierarchy, data flow patterns, state management strategy, and API integration.

**Technology Stack:**
- **Framework**: SvelteKit 2.x
- **Language**: TypeScript
- **Styling**: CSS with custom properties
- **State Management**: Svelte stores
- **Build Tool**: Vite
- **Routing**: File-based routing (SvelteKit)

---

## System Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────┐
│           Browser Client                     │
│                                              │
│  ┌──────────────────────────────────────┐   │
│  │      SvelteKit Application           │   │
│  │                                      │   │
│  │  ┌────────┐ ┌────────┐ ┌────────┐  │   │
│  │  │ Routes │ │Stores  │ │Components│  │   │
│  │  └────────┘ └────────┘ └────────┘  │   │
│  └──────────────────────────────────────┘   │
│              ↓ HTTP/REST                     │
└─────────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────────┐
│         Backend API Server                   │
│         (Rust + Axum)                        │
│                                              │
│  - Indexing Service                          │
│  - Search Service                            │
│  - Storage Service                           │
│  - Watch Service                             │
└─────────────────────────────────────────────┘
```

---

## Component Hierarchy

### Root Layout Structure

```
+layout.svelte (Root Layout)
├── Header
│   ├── Logo
│   ├── MobileMenuToggle
│   ├── Navigation Links
│   │   ├── Dashboard
│   │   ├── Index
│   │   ├── Search
│   │   ├── Entities
│   │   ├── Storage
│   │   ├── Watch
│   │   └── Tools
│   ├── MobileOverlay
│   └── Version Indicator + Offline Status
├── Main Content Area
│   ├── Skip Link (Accessibility)
│   └── Route Content (<slot />)
├── ToastContainer (Global Notifications)
└── Footer
    ├── Project Name
    ├── Links (GitHub, Config)
    └── Credits
```

### Module-Specific Components

#### 1. Dashboard (`/`)
```
+page.svelte
├── Statistics Cards
│   ├── Total Projects
│   ├── Total Files
│   └── Total Entities
├── System Status Indicator
└── Quick Action Buttons
```

#### 2. Index Management (`/index`)
```
+page.svelte
├── AddProjectForm
│   ├── Path Input
│   ├── Name Input
│   ├── Language Filter
│   └── Exclude Patterns
├── ProjectList
│   └── ProjectCard (repeated)
│       ├── Project Info
│       ├── Progress Bar
│       └── Action Buttons
└── IndexStatusPanel
```

#### 3. Search Interface (`/search`)
```
+page.svelte
├── SearchBar
│   ├── Query Input
│   ├── Language Filter
│   └── Entity Type Filter
├── SearchResults
│   └── ResultCard (repeated)
│       ├── Entity Preview
│       ├── Code Snippet
│       └── Metadata
└── PaginationControls
```

#### 4. Entity Explorer (`/entities/[id]`)
```
+page.svelte
├── EntityHeader
│   ├── Entity Name
│   ├── Type Badge
│   └── Location Info
├── SourceCodeViewer
│   └── Syntax Highlighted Code
├── RelationshipTabs
│   ├── Callers List
│   ├── Callees List
│   └── Dependencies List
├── CallGraph (Lazy Loaded)
└── InheritanceTree (Lazy Loaded)
```

#### 5. Storage Management (`/storage`)
```
+page.svelte
├── StorageStatistics
│   ├── Total Usage
│   ├── Language Breakdown Chart
│   └── Database Sizes
├── ClearDataDialog
│   ├── Warning Message
│   ├── Confirmation Input
│   └── Action Buttons
└── CacheManagementPanel
```

#### 6. File Watching (`/watch`)
```
+page.svelte
├── WatchConfiguration
│   ├── Directory Path Input
│   ├── Extension Filters
│   └── Debounce Settings
├── ControlButtons
│   ├── Start/Stop
│   └── Pause/Resume
├── EventFeed
│   └── EventItem (repeated)
│       ├── Timestamp
│       ├── File Path
│       └── Change Type
└── FeedControls
    ├── Pause Feed
    ├── Clear Feed
    └── Auto-scroll Toggle
```

#### 7. Developer Tools (`/tools`)
```
+page.svelte
├── ToolTabs
│   ├── Symbol Lookup Tab
│   ├── Compression Tab
│   └── Diagnostics Tab
├── SymbolLookupPanel
│   ├── Search Input
│   ├── Language Filter
│   └── Results Table
├── CompressionPanel
│   ├── Language Selector
│   ├── Compression Level
│   └── Statistics Display
└── DiagnosticsPanel
    ├── Test Selector
    ├── Run Button
    └── Results Display
```

#### 8. Summary Generation (`/summary`)
```
+page.svelte
├── EntitySelector
├── SummaryOptions
│   ├── Detail Level
│   ├── Format Selector
│   └── Include Examples Toggle
├── GenerateButton
└── SummaryOutput
```

#### 9. Configuration (`/config`)
```
+page.svelte
├── ApiSettings
├── DisplayOptions
├── PerformanceSettings
└── SaveButton
```

---

## Data Flow Architecture

### 1. State Management Strategy

CCE Frontend uses **Svelte stores** for global state management, following a unidirectional data flow pattern.

#### Store Organization

```
src/lib/stores/
├── index.ts          # Index management state
├── search.ts         # Search query and results
├── storage.ts        # Storage statistics
├── watch.ts          # File watching state
├── tools.ts          # Developer tools state
├── toast.ts          # Notification system
└── network.ts        # Network connectivity status
```

#### Store Pattern Example

```typescript
// src/lib/stores/search.ts
import { writable } from 'svelte/store';

export interface SearchState {
  query: string;
  results: SearchResult[];
  loading: boolean;
  error: string | null;
  filters: SearchFilters;
}

export const searchStore = writable<SearchState>({
  query: '',
  results: [],
  loading: false,
  error: null,
  filters: {
    language: null,
    entityType: null
  }
});

export const searchActions = {
  setQuery(query: string) {
    searchStore.update(state => ({ ...state, query }));
  },
  
  async performSearch() {
    searchStore.update(state => ({ ...state, loading: true, error: null }));
    
    try {
      const results = await searchApi.search(searchStore.query);
      searchStore.update(state => ({ 
        ...state, 
        results, 
        loading: false 
      }));
    } catch (error) {
      searchStore.update(state => ({ 
        ...state, 
        error: error.message, 
        loading: false 
      }));
    }
  }
};
```

### 2. Component Communication

#### Parent-to-Child: Props
```svelte
<!-- Parent -->
<SearchResults results={results} on:resultClick={handleClick} />

<!-- Child -->
<script lang="ts">
  export let results: SearchResult[];
  export let onresultClick: (result: SearchResult) => void;
</script>
```

#### Child-to-Parent: Events
```svelte
<!-- Child Component -->
<button on:click={() => dispatch('select', item)}>
  Select
</button>

<!-- Parent Component -->
<Component on:select={handleSelect} />
```

#### Global State: Stores
```typescript
// Any component can subscribe
import { searchStore } from '$lib/stores/search';

$: results = $searchStore.results;
```

### 3. API Integration Pattern

All API calls go through the centralized `ApiClient` with retry logic.

```typescript
// src/lib/api/client.ts
export class ApiClient {
  private baseUrl: string;
  
  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }
  
  async get<T>(endpoint: string): Promise<T> {
    return this.fetchWithRetry(`${this.baseUrl}${endpoint}`);
  }
  
  async post<T>(endpoint: string, data: any): Promise<T> {
    return this.fetchWithRetry(`${this.baseUrl}${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data)
    });
  }
  
  private async fetchWithRetry<T>(
    url: string,
    options: RequestInit = {},
    retries = 3
  ): Promise<T> {
    // Exponential backoff retry logic
    // ... implementation ...
  }
}

export const apiClient = new ApiClient(import.meta.env.PUBLIC_API_URL || 'http://localhost:9000');
```

### 4. Data Flow Examples

#### Example 1: Search Flow

```
User Types Query
    ↓
SearchInput Component
    ↓
searchActions.setQuery()
    ↓
searchStore updated
    ↓
User Clicks Search
    ↓
searchActions.performSearch()
    ↓
apiClient.get('/api/search')
    ↓
Backend Processing
    ↓
Response Received
    ↓
searchStore.results updated
    ↓
SearchResults Component Re-renders
    ↓
User Sees Results
```

#### Example 2: File Watching Flow

```
User Clicks "Start Watching"
    ↓
WatchConfiguration Component
    ↓
watchActions.startWatching(config)
    ↓
apiClient.post('/api/watch/start')
    ↓
Backend Starts File Monitor
    ↓
WebSocket/SSE Connection Established
    ↓
File Change Detected
    ↓
Event Pushed to Frontend
    ↓
watchStore.events updated
    ↓
EventFeed Component Re-renders
    ↓
User Sees Real-time Updates
```

#### Example 3: Entity Navigation Flow

```
User Clicks Search Result
    ↓
ResultCard Component emits event
    ↓
Router navigates to /entities/[id]
    ↓
EntityPage loads
    ↓
entityActions.loadEntity(id)
    ↓
apiClient.get(`/api/entity/${id}`)
    ↓
Backend Returns Entity Data
    ↓
entityStore.entity updated
    ↓
EntityHeader, SourceCodeViewer render
    ↓
User Views Entity Details
```

---

## Lazy Loading & Code Splitting

### Strategy

Heavy components are lazy-loaded using dynamic imports to reduce initial bundle size.

### Implementation Pattern

```typescript
// In route component
<script lang="ts">
  import { onMount } from 'svelte';
  
  let CallGraph: any = null;
  let graphLoaded = false;
  
  async function loadCallGraph() {
    if (!graphLoaded) {
      const module = await import('$lib/components/entities/CallGraph.svelte');
      CallGraph = module.default;
      graphLoaded = true;
    }
  }
  
  // Load when component mounts or when needed
  onMount(() => {
    loadCallGraph();
  });
</script>

{#if CallGraph}
  <svelte:component this={CallGraph} entityId={entityId} />
{:else}
  <div class="loading-spinner">Loading call graph...</div>
{/if}
```

### Components Using Lazy Loading

1. **CallGraph.svelte** - D3.js visualization
2. **InheritanceTree.svelte** - Tree visualization
3. **LogViewer.svelte** - Large log display

### Benefits

- Reduced initial JavaScript payload
- Faster Time to Interactive (TTI)
- Better performance on low-end devices
- Improved Lighthouse scores

---

## Responsive Design Architecture

### Breakpoint Strategy

```css
/* Mobile First Approach */
/* Base styles: Mobile (< 768px) */

@media (min-width: 768px) {
  /* Tablet */
}

@media (min-width: 1024px) {
  /* Desktop */
}

@media (max-width: 480px) {
  /* Small mobile optimizations */
}
```

### Responsive Patterns

#### 1. Grid Layouts
```css
.grid-container {
  display: grid;
  grid-template-columns: 1fr; /* Mobile: single column */
  gap: 1rem;
}

@media (min-width: 1024px) {
  .grid-container {
    grid-template-columns: repeat(3, 1fr); /* Desktop: 3 columns */
  }
}
```

#### 2. Mobile Navigation
```svelte
<!-- Hamburger menu for mobile -->
<button class="mobile-menu-toggle" on:click={toggleMobileMenu}>
  <span class="hamburger-icon"></span>
</button>

<nav class:open={mobileMenuOpen}>
  <!-- Navigation links -->
</nav>

<div class="mobile-overlay" on:click={closeMobileMenu}></div>
```

#### 3. Responsive Tables
```css
/* Desktop: Traditional table */
.table {
  display: table;
}

/* Mobile: Card-based layout */
@media (max-width: 480px) {
  .table-row {
    display: block;
    margin-bottom: 1rem;
    border: 1px solid var(--gray-200);
  }
  
  .table-cell::before {
    content: attr(data-label);
    font-weight: bold;
  }
}
```

#### 4. Touch Targets
```css
@media (max-width: 768px) {
  button, input, select {
    min-height: 44px; /* WCAG 2.5.5 compliance */
    min-width: 44px;
  }
}
```

---

## Accessibility Architecture

### WCAG 2.1 AA Compliance

#### 1. Semantic HTML
```svelte
<!-- Correct -->
<nav aria-label="Main navigation">
  <a href="/">Home</a>
</nav>

<main id="main-content">
  <!-- Page content -->
</main>
```

#### 2. ARIA Attributes
```svelte
<button 
  aria-label="Close dialog"
  aria-expanded={isOpen}
  on:click={close}
>
  ×
</button>

<div role="dialog" aria-modal="true" aria-labelledby="dialog-title">
  <h2 id="dialog-title">Confirm Action</h2>
</div>
```

#### 3. Keyboard Navigation
```svelte
<!-- Focus trap in dialogs -->
<div 
  tabindex="-1"
  on:keydown={(e) => {
    if (e.key === 'Escape') close();
  }}
>
  <!-- Dialog content -->
</div>

<!-- Skip link -->
<a href="#main-content" class="skip-link">
  Skip to main content
</a>
```

#### 4. Live Regions
```svelte
<!-- Announce dynamic updates -->
<div aria-live="polite" aria-atomic="true" class="sr-only">
  {statusMessage}
</div>
```

#### 5. Form Labels
```svelte
<!-- Associated labels -->
<label for="search-query">Search</label>
<input id="search-query" type="text" bind:value={query} />

<!-- Or wrapped -->
<label>
  Search
  <input type="text" bind:value={query} />
</label>
```

---

## Error Handling Architecture

### Centralized Error Management

#### 1. API Error Handling
```typescript
// src/lib/api/client.ts
async function handleResponse(response: Response) {
  if (!response.ok) {
    const error = await response.json().catch(() => ({}));
    throw new ApiError({
      status: response.status,
      message: error.message || response.statusText,
      code: error.code
    });
  }
  return response.json();
}
```

#### 2. Component Error Boundaries
```svelte
<script lang="ts">
  import { onError } from 'svelte';
  
  let error: Error | null = null;
  
  onError((e) => {
    error = e;
    toastActions.error('Failed to load component');
  });
</script>

{#if error}
  <div class="error-state">
    <p>An error occurred</p>
    <button on:click={retry}>Retry</button>
  </div>
{:else}
  <slot />
{/if}
```

#### 3. Toast Notification System
```typescript
// src/lib/stores/toast.ts
export const toastActions = {
  success(message: string) {
    toasts.update(state => [...state, {
      id: generateId(),
      message,
      type: 'success',
      duration: 5000
    }]);
  },
  
  error(message: string) {
    toasts.update(state => [...state, {
      id: generateId(),
      message,
      type: 'error',
      duration: 10000
    }]);
  }
};
```

---

## Performance Optimization Architecture

### 1. Bundle Analysis
```typescript
// vite.config.ts
import { visualizer } from 'rollup-plugin-visualizer';

export default defineConfig({
  plugins: [
    sveltekit(),
    visualizer({
      open: true,
      filename: 'stats.html',
      gzipSize: true,
      brotliSize: true
    })
  ]
});
```

### 2. Caching Strategy

#### HTTP Caching (Backend)
```rust
// Rust backend sets cache headers
Cache-Control: public, max-age=3600
```

#### Client-Side Caching
```typescript
// In-memory cache with TTL
const CACHE_TTL = 30000; // 30 seconds
let cache = new Map();

async function getCachedData(key: string, fetchFn: () => Promise<any>) {
  const cached = cache.get(key);
  if (cached && Date.now() - cached.timestamp < CACHE_TTL) {
    return cached.data;
  }
  
  const data = await fetchFn();
  cache.set(key, { data, timestamp: Date.now() });
  return data;
}
```

### 3. Debouncing & Throttling
```typescript
// Debounce search input
import { debounce } from '$lib/utils/debounce';

const debouncedSearch = debounce((query: string) => {
  searchActions.performSearch(query);
}, 300);

<input on:input={(e) => debouncedSearch(e.target.value)} />
```

---

## Testing Architecture

### Unit Testing
```typescript
// Component unit tests
import { render, fireEvent } from '@testing-library/svelte';
import SearchInput from './SearchInput.svelte';

test('emits search event on enter', async () => {
  const { getByRole } = render(SearchInput);
  const input = getByRole('textbox');
  
  await fireEvent.keyDown(input, { key: 'Enter' });
  
  expect(onSearch).toHaveBeenCalled();
});
```

### Integration Testing
```typescript
// E2E workflow tests
import { test, expect } from '@playwright/test';

test('complete search workflow', async ({ page }) => {
  await page.goto('/search');
  await page.fill('[role="searchbox"]', 'authentication');
  await page.click('[type="submit"]');
  
  await expect(page.locator('.result-card')).toHaveCountGreaterThan(0);
});
```

---

## Deployment Architecture

### Docker Setup
```dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/build /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

### Nginx Configuration
```nginx
server {
  listen 80;
  server_name cce.example.com;
  
  root /usr/share/nginx/html;
  index index.html;
  
  # SPA routing
  location / {
    try_files $uri $uri/ /index.html;
  }
  
  # API proxy
  location /api/ {
    proxy_pass http://backend:9000;
    proxy_set_header Host $host;
  }
  
  # Cache static assets
  location ~* \.(js|css|png|jpg)$ {
    expires 1y;
    add_header Cache-Control "public, immutable";
  }
}
```

---

## Monitoring & Analytics

### Error Tracking (Optional)
```typescript
// Sentry integration
import * as Sentry from '@sentry/svelte';

Sentry.init({
  dsn: process.env.SENTRY_DSN,
  environment: process.env.NODE_ENV
});
```

### Performance Metrics
```typescript
// Web Vitals monitoring
import { getCLS, getFID, getLCP } from 'web-vitals';

export function reportWebVitals() {
  getCLS(console.log);
  getFID(console.log);
  getLCP(console.log);
}
```

---

## Future Enhancements

### Planned Improvements

1. **State Persistence**
   - Persist user preferences to localStorage
   - Restore previous session state

2. **Offline Support**
   - Service Worker implementation
   - IndexedDB for offline data storage

3. **Real-time Collaboration**
   - WebSocket for live updates
   - Multi-user awareness

4. **Advanced Caching**
   - SWR (Stale-While-Revalidate) pattern
   - Predictive prefetching

5. **Plugin Architecture**
   - Extensible tool system
   - Custom visualization plugins

---

## Conclusion

The CCE Frontend architecture emphasizes:

- **Modularity**: Clear separation of concerns
- **Performance**: Code splitting and lazy loading
- **Accessibility**: WCAG 2.1 AA compliance
- **Maintainability**: TypeScript, consistent patterns
- **Scalability**: Modular stores and components

This architecture supports current requirements while providing flexibility for future enhancements.

---

**Last Updated**: 2026-05-02  
**Document Version**: 1.0  
**Application Version**: v0.1.0
