# Frontend Functional Analysis - Code Context Engine Web UI

## Context

This plan outlines the implementation of a web-based frontend for the Code Context Engine (CCE) project, transforming its existing command-line interface into an interactive web application. The goal is to expose core CCE functionality through a modern, Swiss Minimalist / Tech Industrial design system using Svelte as the primary technology stack.

The analysis derives feature requirements from the existing CLI commands in `cce-cli/src/commands/` and maps them to corresponding web UI modules while adhering to strict design constraints defined in `docs/frontend/style/`.

---

## Feature Mapping: CLI Commands → Web UI Modules

### 1. **Dashboard Module** (Status + Metrics)
**CLI Counterparts:**
- `cce-cli status` - Server health check and storage overview
- `cce-cli metrics json` - Export JSON metrics
- `cce-cli metrics prometheus` - Prometheus metrics export

**Web UI Features:**
- Real-time server connectivity indicator
- Storage component health cards (Vector DB, BM25, SQLite, Cache)
- System metrics dashboard with charts:
  - Index statistics (files indexed, entities processed)
  - Query performance (latency percentiles, result counts)
  - Cache hit rates
  - Resource usage indicators
- Auto-refresh mechanism (polling every 30s via Svelte stores)

**Design Application:**
- Use 4-column spec bar layout (`grid-template-columns: repeat(4, 1fr)`) for health cards
- Apply Space Mono labels with uppercase styling for metric names
- Display values in bold Space Grotesk typography
- Use accent color (#ff3d00) sparingly for critical alerts only
- Sharp borders (1px solid black), no shadows, no border-radius
- High contrast black-on-white for data tables

---

### 2. **Index Management Module** (Core Priority)
**CLI Counterparts:**
- `cce-cli index run` - Full directory indexing
- `cce-cli index incremental` - Incremental indexing
- `cce-cli index parse` - Single file parse preview
- `cce-cli project create/list/get/update/delete` - Project lifecycle management
- `cce-cli project index <ID>` - Trigger project indexing

**Web UI Features:**
- **Project Manager Panel:**
  - Project list table with status badges
  - Create/edit/delete project forms
  - Project configuration editor (extensions, exclude patterns)
  - Quick index trigger button per project
  
- **Index Control Center:**
  - Start full/incremental index with parameter inputs:
    - Directory path selector
    - File extension filters (checkboxes)
    - Exclude directory patterns
    - Force re-index toggle
    - Gitignore respect toggle
  - Real-time progress tracking:
    - Progress bar with percentage
    - Current file being processed
    - Phase indicators (scan → parse → embed → store)
    - Error count display
  - Pause/resume/cancel controls
  - Index history log (timestamped operations)

- **File Parser Preview:**
  - Upload/select single file for parsing
  - Display extracted entities in structured format
  - Show AST-to-NL conversion output
  - Language detection indicator

**Design Application:**
- Two-column grid layout for project list vs. detail view
- Black background CTA buttons with hover transform to accent color (#ff3d00)
- Form inputs with sharp borders, Space Mono labels
- Progress bars as thick black lines filling left-to-right
- Tables with 1px gray-200 grid lines, high-density information
- Status badges using accent color for active states
- Code blocks with black background, white text, Space Mono font

---

### 3. **Search Interface Module**
**CLI Counterparts:**
- `cce-cli search query` - Advanced code search with multiple query types

**Web UI Features:**
- **Search Input Panel:**
  - Large search input field with typeahead suggestions
  - Query type selector tabs: Vector | BM25 | Hybrid | Hierarchical
  - Advanced filter panel (collapsible):
    - Directory prefix filter
    - File extension checkboxes
    - Entity type filters (function, class, etc.)
    - Language filters
    - Content type selector (code, code+docs, all)
    - Include/exclude pattern inputs
    - Min score threshold slider
    - Pagination controls (page size, offset)
  
- **Results Display:**
  - Result cards showing:
    - Code snippet preview (syntax-highlighted)
    - Relevance score badge
    - File path with line numbers
    - Entity metadata (type, name, language)
    - Call chain depth indicator (if applicable)
  - Sort options: relevance, file path, entity type
  - Click-to-expand for full context
  - Copy code button per result

**Design Application:**
- Hero-style large search input with bold typography (Space Grotesk, 2rem+)
- Filter panel with grid layout, compact spacing
- Result cards with sharp borders, hover state shifts to gray-100
- Code snippets in black background blocks with Space Mono
- Score badges using accent color for high-relevance items
- Underline animation on clickable elements (nav-style hover effect)
- Information-dense layout minimizing whitespace

---

### 4. **Entity Explorer Module**
**CLI Counterparts:**
- `cce-cli entity function <ID>` - Function details
- `cce-cli entity calls/callers <ID>` - Call relationships
- `cce-cli entity call-chain <ID>` - Full call chain visualization
- `cce-cli entity call-path` - Path between functions
- `cce-cli entity inheritance <ID>` - Class inheritance tree
- `cce-cli entity implementations <ID>` - Interface implementations

**Web UI Features:**
- **Entity Detail View:**
  - Function/class signature display
  - Source code location link
  - Metadata panel (language, file path, line range)
  - Natural language description (from AST-to-NL)
  
- **Relationship Graph Visualization:**
  - Interactive call graph (SVG-based):
    - Nodes represent functions/classes
    - Directed edges show call relationships
    - Zoom/pan controls
    - Node click reveals detail panel
  - Inheritance tree diagram (vertical/horizontal toggle)
  - Implementation hierarchy viewer
  
- **Call Chain Explorer:**
  - Linear chain display with direction toggle (up/down)
  - Each step shows function name, file, line number
  - Click any step to navigate to that entity
  - Depth control slider

**Design Application:**
- SVG-based diagrams using stroke-dasharray animation for edge drawing
- Nodes as rectangular cards with sharp borders, accent color left-border for user-selected nodes
- Staggered node pop-in animations (cubic-bezier easing, fast duration)
- Grid layout for detail panels (2-column: metadata | code)
- Space Mono for technical identifiers, Space Grotesk for descriptions
- Navigation breadcrumbs with underline hover animation
- High contrast black lines for graph edges

---

### 5. **Storage Management Module**
**CLI Counterparts:**
- `cce-cli storage status` - Storage component status
- `cce-cli storage stats` - Index statistics
- `cce-cli storage clear` - Clear index data (selective)
- `cce-cli storage delete-file <PATH>` - Delete single file
- `cce-cli storage delete-entity <ID>` - Delete single entity
- `cce-cli storage batch-delete` - Batch deletion

**Web UI Features:**
- **Storage Overview Dashboard:**
  - Component health cards (Qdrant, BM25, SQLite, Cache)
  - Usage statistics:
    - Vector count and dimension
    - BM25 document count
    - Relation graph node/edge counts
    - Cache hit/miss rates
    - Total disk usage breakdown
  
- **Selective Cleanup Interface:**
  - Checkbox selectors for clear targets:
    - Vectors
    - BM25 index
    - Relations
    - Cache
  - Confirmation dialog before destructive operations
  - Batch delete form:
    - Multi-select file/entity lists
    - Search/filter available items
    - Execute batch operation button

**Design Application:**
- Spec bar layout (4-column grid) for component status
- Warning dialogs with accent color borders for destructive actions
- Data tables with alternating row backgrounds (white/gray-100)
- Action buttons in black with hover displacement effect
- Numerical values in bold Space Grotesk, labels in Space Mono uppercase
- Strict alignment using CSS grid, no floating elements

---

### 6. **File Watching Module**
**CLI Counterparts:**
- `cce-cli watch start` - Start file watching
- `cce-cli watch stop` - Stop file watching
- `cce-cli watch status` - Get watch status

**Web UI Features:**
- **Watch Control Panel:**
  - Toggle switch to start/stop watching
  - Directory selection input (multiple paths)
  - Extension filter configuration
  - Debounce interval slider (100ms - 5000ms)
  
- **Live Event Feed:**
  - Real-time log of file system events:
    - Timestamp
    - Event type (create, modify, delete)
    - File path
    - Action taken (queued for indexing, ignored)
  - Auto-scroll to latest event
  - Pause/resume feed button
  - Event count summary

**Design Application:**
- Toggle switches styled as black rectangles sliding to reveal accent color
- Event feed as monospace log display (Space Mono, black background, white text)
- Live indicator dot pulsing in accent color when active
- Compact card layout with sharp borders
- Fast, mechanical animations for state transitions

---

### 7. **Tools Module**
**CLI Counterparts:**
- `cce-cli tools compress` - Code compression
- `cce-cli tools batch-compress` - Batch compression
- `cce-cli tools diagnose` - Code diagnosis
- `cce-cli tools symbols` - Symbol extraction
- `cce-cli tools references` - Find references
- `cce-cli tools definition` - Go to definition

**Web UI Features:**
- **Code Compression Tool:**
  - Textarea for code input
  - Language selector dropdown
  - Compress button
  - Output panel showing compressed version
  - Token count reduction statistics
  
- **Code Diagnosis Tool:**
  - Code input area
  - Diagnose button
  - Results panel listing:
    - Issue severity (error, warning, info)
    - Description
    - Suggested fix
    - Location (line/column)
  
- **Symbol Lookup Tools:**
  - File path input or upload
  - Extract symbols button
  - Symbol list display (type, name, location)
  - Reference/definition search results

**Design Application:**
- Split-pane layout (input | output) with draggable divider
- Code editors with black background, white text, Space Mono
- Result panels with numbered lists, sharp borders
- Severity badges using accent color for errors
- Button groups aligned horizontally with consistent spacing
- High information density, minimal decorative elements

---

### 8. **Configuration Module**
**CLI Counterparts:**
- `cce-cli config reload` - Reload configuration

**Web UI Features:**
- **Config Editor:**
  - TOML syntax-highlighted editor for global config
  - Project-specific config override panel
  - Environment variable manager (.env file editor)
  - Validation feedback (error highlighting)
  
- **Hot Reload Controls:**
  - Reload configuration button
  - Reload status indicator (success/failure)
  - Last reload timestamp

**Design Application:**
- Code editor with black background, syntax highlighting using gray scale + accent
- Config sections separated by 1px black borders
- Validation errors highlighted with accent color left-border
- Action buttons in standard CTA style (black bg, white text, hover to accent)
- Monospace labels for config keys, proportional font for descriptions

---

### 9. **Summary Generation Module**
**CLI Counterparts:**
- `cce-cli summary generate` - Generate file summaries

**Web UI Features:**
- **Summary Generator:**
  - File/directory selector (multi-select)
  - Configuration options:
    - Extension filters
    - Exclude patterns
    - Max files limit
    - Gitignore respect toggle
  - Generate button
  - Summary output panel:
    - Per-file natural language summary
    - Copy to clipboard button
    - Download as markdown option

**Design Application:**
- Form layout with grid alignment
- Output cards with summary text in readable Space Grotesk body style
- Action buttons grouped at bottom with consistent spacing
- Clean separation between input and output sections using borders

---

## Technology Stack & Constraints

### Core Technologies
- **Framework:** Svelte 5 (latest stable)
- **Build Tool:** Vite
- **Styling:** Pure CSS (no preprocessors, no frameworks)
- **State Management:** Svelte stores (writable, readable, derived)
- **HTTP Client:** Native Fetch API (no Axios or similar)
- **Routing:** SvelteKit file-based routing or minimal custom router

### Dependency Minimization Strategy
- **Avoid:** Heavy UI libraries (Material UI, Ant Design, Bootstrap)
- **Avoid:** Complex state managers (Redux, Zustand)
- **Avoid:** Icon libraries (use inline SVGs or Unicode characters)
- **Prefer:** Native browser APIs (IntersectionObserver, ResizeObserver)
- **Prefer:** CSS Grid/Flexbox for layouts (no grid frameworks)
- **Prefer:** Custom components built from scratch

### Allowed Lightweight Dependencies (if absolutely necessary)
- Syntax highlighting: `prismjs` (minimal setup)
- Charts: Custom SVG charts or lightweight library like `chart.js` (only if complex visualizations needed)
- Date formatting: Native `Intl.DateTimeFormat`

---

## Design System Implementation Guidelines

### Color Palette (Strict Enforcement)
```css
:root {
  --black: #0a0a0a;
  --white: #fafafa;
  --accent: #ff3d00;
  --gray-100: #f5f5f5;
  --gray-200: #e5e5e5;
  --gray-400: #a3a3a3;
  --gray-600: #525252;
  --gray-800: #262626;
}
```

**Rules:**
- No custom colors beyond this palette
- Accent color used ONLY for: critical alerts, active states, emphasis markers
- Primary visual language: black-and-white high contrast
- Gray scale for secondary text, borders, backgrounds

### Typography
- **Headings:** Space Grotesk, weight 700, letter-spacing -0.03em to -0.04em, line-height 1.05-1.1
- **Body:** Space Grotesk, size 1rem-1.25rem, color var(--gray-600), line-height 1.6-1.7
- **Labels/Meta:** Space Mono, size 0.65rem-0.75rem, uppercase, letter-spacing 0.1em
- **Code:** Space Mono, size 0.85rem, background var(--black), color var(--white)

### Layout Principles
- **Container:** max-width 1400px, margin 0 auto, padding 0 2rem
- **Grid System:** CSS Grid exclusively, modular divisions (1fr 1fr, repeat(4, 1fr), etc.)
- **Borders:** 1px solid black for section boundaries, 1px solid gray-200 for internal dividers
- **Corners:** Zero border-radius everywhere
- **Shadows:** None (flat design only)
- **Spacing:** Section padding 6rem 0, component gaps 2rem standard

### Component Patterns
- **Buttons:** Black background, white text, Space Mono, uppercase, hover transforms to accent color with translateX(4px) displacement
- **Cards:** Sharp corners, 1px black borders, no shadows, hover shifts background to gray-100
- **Navigation Links:** Space Mono, uppercase, underline slide-in animation on hover
- **Tables:** Grid-based, 1px gray-200 lines, high information density
- **Forms:** Sharp-bordered inputs, Space Mono labels, compact vertical spacing

### Animation Specifications
- **Line Drawing:** SVG stroke-dasharray/stroke-dashoffset for graph edges
- **Node Pop-in:** Opacity 0→1, translateY(10px)→0, cubic-bezier(0.2, 0.9, 0.2, 1), 0.55s duration
- **Button Hover:** translateX(4px), 0.3s transition
- **Underline Slide:** scaleX(0)→scaleX(1), 0.3s transition
- **General Principle:** Fast, crisp, mechanical feel (avoid slow, bouncy, organic easing)

### Responsive Breakpoints
- **> 1024px:** Full multi-column layouts
- **≤ 1024px:** Stack hero sections vertically, features become single column, specs become 2-column
- **≤ 768px:** Header collapses to single column, navigation centered, footer stacked

---

## Critical Files to Modify/Create

### Backend (Minimal Changes Required)
- **No backend changes needed** - existing REST API endpoints are sufficient
- Ensure CORS headers are properly configured in Axum server for local development

### Frontend Structure (New Files)
```
frontend/
├── src/
│   ├── lib/
│   │   ├── stores/
│   │   │   ├── index.ts          # Index state store
│   │   │   ├── search.ts         # Search state store
│   │   │   ├── projects.ts       # Project management store
│   │   │   ├── metrics.ts        # Metrics data store
│   │   │   └── watch.ts          # File watch state store
│   │   ├── api/
│   │   │   ├── client.ts         # Fetch wrapper with base URL
│   │   │   ├── index.ts          # Index API calls
│   │   │   ├── search.ts         # Search API calls
│   │   │   ├── projects.ts       # Project API calls
│   │   │   ├── entities.ts       # Entity API calls
│   │   │   ├── storage.ts        # Storage API calls
│   │   │   ├── watch.ts          # Watch API calls
│   │   │   ├── tools.ts          # Tools API calls
│   │   │   └── metrics.ts        # Metrics API calls
│   │   ├── components/
│   │   │   ├── ui/
│   │   │   │   ├── Button.svelte
│   │   │   │   ├── Card.svelte
│   │   │   │   ├── Input.svelte
│   │   │   │   ├── Badge.svelte
│   │   │   │   ├── ProgressBar.svelte
│   │   │   │   └── CodeBlock.svelte
│   │   │   ├── dashboard/
│   │   │   │   ├── HealthCard.svelte
│   │   │   │   ├── MetricChart.svelte
│   │   │   │   └── StatusIndicator.svelte
│   │   │   ├── index/
│   │   │   │   ├── ProjectList.svelte
│   │   │   │   ├── IndexProgress.svelte
│   │   │   │   └── FileParser.svelte
│   │   │   ├── search/
│   │   │   │   ├── SearchInput.svelte
│   │   │   │   ├── FilterPanel.svelte
│   │   │   │   └── ResultCard.svelte
│   │   │   ├── entities/
│   │   │   │   ├── EntityDetail.svelte
│   │   │   │   ├── CallGraph.svelte
│   │   │   │   └── InheritanceTree.svelte
│   │   │   └── ... (other module components)
│   │   └── utils/
│   │       ├── formatters.ts     # Date, number formatting
│   │       └── validators.ts     # Input validation
│   ├── routes/
│   │   ├── +layout.svelte        # Root layout with nav
│   │   ├── +page.svelte          # Dashboard home
│   │   ├── index/
│   │   │   └── +page.svelte      # Index management page
│   │   ├── search/
│   │   │   └── +page.svelte      # Search interface page
│   │   ├── entities/
│   │   │   ├── +page.svelte      # Entity list/search
│   │   │   └── [id]/
│   │   │       └── +page.svelte  # Entity detail page
│   │   ├── storage/
│   │   │   └── +page.svelte      # Storage management page
│   │   ├── watch/
│   │   │   └── +page.svelte      # File watching page
│   │   ├── tools/
│   │   │   └── +page.svelte      # Tools playground page
│   │   ├── config/
│   │   │   └── +page.svelte      # Configuration editor page
│   │   └── summary/
│   │       └── +page.svelte      # Summary generator page
│   ├── app.html                  # HTML template
│   ├── app.css                   # Global styles (design tokens)
│   └── hooks.server.ts           # Optional server hooks
├── static/
│   └── favicon.png
├── svelte.config.js
├── vite.config.ts
├── package.json
└── README.md
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1)
1. Initialize SvelteKit project with Vite
2. Set up design system (CSS variables, typography, base components)
3. Implement API client layer with fetch wrapper
4. Create core Svelte stores for state management
5. Build root layout with navigation (header, footer)

### Phase 2: Core Features (Week 2-3)
1. **Dashboard Module:**
   - Health status cards
   - Metrics display
   - Auto-refresh mechanism
2. **Index Management Module:**
   - Project CRUD operations
   - Index trigger with parameters
   - Real-time progress tracking
   - File parser preview

### Phase 3: Search & Entities (Week 4-5)
1. **Search Interface:**
   - Search input with filters
   - Results display with code snippets
   - Pagination and sorting
2. **Entity Explorer:**
   - Entity detail views
   - SVG-based call graph visualization
   - Inheritance tree diagrams
   - Call chain explorer

### Phase 4: Advanced Features (Week 6-7)
1. **Storage Management:**
   - Storage overview dashboard
   - Selective cleanup interface
   - Batch delete functionality
2. **File Watching:**
   - Watch control panel
   - Live event feed
3. **Tools Module:**
   - Code compression tool
   - Diagnosis tool
   - Symbol lookup tools

### Phase 5: Polish & Optimization (Week 8)
1. Responsive design adjustments for tablet/mobile
2. Performance optimization (lazy loading, code splitting)
3. Accessibility improvements (ARIA labels, keyboard navigation)
4. Error handling and user feedback
5. Documentation and deployment setup

---

## Verification & Testing

### Manual Testing Checklist
1. **Server Connectivity:**
   - Verify frontend can reach backend API at configured URL
   - Test CORS preflight requests succeed
   
2. **Index Management:**
   - Create new project with valid/invalid paths
   - Trigger full index and observe real-time progress updates
   - Cancel ongoing index operation
   - Parse single file and verify entity extraction display
   
3. **Search:**
   - Execute vector/bm25/hybrid searches
   - Apply filters (directory, extensions, entity types)
   - Verify result accuracy and relevance scores
   - Test pagination with large result sets
   
4. **Entity Explorer:**
   - Navigate to function/class detail pages
   - Render call graph SVG correctly
   - Traverse inheritance trees
   - Follow call chains in both directions
   
5. **Storage Management:**
   - View storage statistics
   - Perform selective clear operations
   - Verify data deletion reflects in subsequent queries
   
6. **File Watching:**
   - Start/stop watch operations
   - Observe live event feed updates
   - Configure debounce intervals
   
7. **Responsive Design:**
   - Test layouts at 1920px, 1024px, 768px, 375px widths
   - Verify grid collapses appropriately
   - Check touch targets on mobile (>44px)

### Automated Testing Strategy
1. **Unit Tests (Vitest):**
   - Test Svelte stores state transitions
   - Test utility functions (formatters, validators)
   - Test API client error handling
   
2. **Component Tests (Testing Library):**
   - Test UI components render correctly with props
   - Test user interactions (button clicks, form submissions)
   - Test accessibility attributes
   
3. **Integration Tests (Playwright):**
   - End-to-end workflows (create project → index → search)
   - API integration verification
   - Cross-browser compatibility (Chrome, Firefox, Safari)

### Performance Benchmarks
- Initial page load < 2s on 3G connection
- Search query response < 500ms (excluding backend processing)
- Smooth animations at 60fps
- No layout shifts during content loading

---

## Risk Mitigation

### Potential Challenges
1. **Real-time Updates:** Polling may cause unnecessary server load
   - *Mitigation:* Implement exponential backoff, consider WebSocket upgrade later

2. **Large Dataset Rendering:** Rendering thousands of search results may lag
   - *Mitigation:* Virtual scrolling, pagination, lazy loading

3. **SVG Graph Complexity:** Complex call graphs may be slow to render
   - *Mitigation:* Limit initial depth, progressive rendering, canvas fallback

4. **CORS Issues:** Browser security may block API requests
   - *Mitigation:* Configure proper CORS headers in Axum, use proxy in dev

5. **Design Consistency:** Maintaining strict Swiss style across components
   - *Mitigation:* Create comprehensive component library, enforce design tokens

---

## Success Criteria

1. ✅ All 9 core modules implemented and functional
2. ✅ 100% of CLI features accessible via web UI
3. ✅ Design system strictly adheres to Swiss Minimalist / Tech Industrial style
4. ✅ Responsive across desktop, tablet, mobile breakpoints
5. ✅ Real-time updates work reliably (index progress, watch events)
6. ✅ Performance meets benchmarks (<2s load, <500ms interactions)
7. ✅ Zero external dependencies beyond SvelteKit ecosystem
8. ✅ All API endpoints tested and verified
9. ✅ Accessibility standards met (WCAG 2.1 AA minimum)
10. ✅ Documentation complete (setup guide, user manual, API reference)

---

## Next Steps

Upon approval of this plan:
1. Initialize SvelteKit project structure
2. Set up design system foundation (CSS variables, typography imports)
3. Implement API client layer
4. Begin Phase 1 implementation (Foundation)
