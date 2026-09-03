# Performance Benchmarking Guide

## Overview

This guide provides instructions for benchmarking the Code Context Engine Frontend performance using industry-standard tools and metrics.

---

## Performance Targets

### Core Web Vitals Targets

| Metric | Target | Good | Needs Improvement | Poor |
|--------|--------|------|-------------------|------|
| **LCP** (Largest Contentful Paint) | < 2.5s | 0-2.5s | 2.5-4.0s | > 4.0s |
| **FID** (First Input Delay) | < 100ms | 0-100ms | 100-300ms | > 300ms |
| **CLS** (Cumulative Layout Shift) | < 0.1 | 0-0.1 | 0.1-0.25 | > 0.25 |
| **TTI** (Time to Interactive) | < 3.0s | - | - | - |
| **TBT** (Total Blocking Time) | < 200ms | 0-200ms | 200-600ms | > 600ms |

### Bundle Size Targets

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Initial JS Bundle (gzipped) | < 200KB | TBD | ⏳ Pending |
| Total App Size (gzipped) | < 500KB | TBD | ⏳ Pending |
| Largest Chunk | < 100KB | TBD | ⏳ Pending |

---

## Benchmarking Tools

### 1. Lighthouse (Primary Tool)

**Installation:**
```bash
# Built into Chrome DevTools
# Or install CLI version
npm install -g lighthouse
```

**Running Lighthouse:**

**Option A: Chrome DevTools**
1. Open Chrome DevTools (F12)
2. Navigate to "Lighthouse" tab
3. Select categories: Performance, Accessibility, Best Practices, SEO
4. Choose device: Mobile or Desktop
5. Click "Analyze page load"
6. Review report

**Option B: CLI**
```bash
# Audit specific URL
lighthouse http://localhost:5173 --view

# Save JSON report
lighthouse http://localhost:5173 --output json --output-path ./report.json

# Mobile emulation
lighthouse http://localhost:5173 --emulated-form-factor=mobile --view
```

**Key Metrics to Monitor:**
- Performance Score (target: > 90)
- Accessibility Score (target: > 95)
- Best Practices Score (target: > 90)
- SEO Score (target: > 90)

### 2. WebPageTest (Advanced Analysis)

**URL:** https://www.webpagetest.org/

**Configuration:**
- Test Location: Choose region closest to users
- Browser: Chrome (latest)
- Connection: 3G Fast, 4G, Cable
- Runs: 3 (for consistency)

**Metrics Captured:**
- First View (cold cache)
- Repeat View (warm cache)
- Waterfall chart
- Filmstrip view
- Video capture

### 3. Bundle Analyzer

**Already configured in project:**
```bash
# Build with visualization
npm run build

# Open stats.html
open stats.html  # macOS
start stats.html  # Windows
xdg-open stats.html  # Linux
```

**What to Look For:**
- Largest dependencies
- Duplicate packages
- Unused code
- Tree-shaking effectiveness

### 4. Chrome DevTools Performance Tab

**Profiling Steps:**
1. Open DevTools → Performance tab
2. Click "Record" button
3. Perform user actions (navigation, search, etc.)
4. Stop recording
5. Analyze timeline

**Key Insights:**
- JavaScript execution time
- Rendering performance
- Layout shifts
- Long tasks (> 50ms)

### 5. Network Panel Analysis

**Steps:**
1. Open DevTools → Network tab
2. Disable cache
3. Reload page
4. Analyze waterfall

**Metrics:**
- Total requests
- Total transfer size
- Time to first byte (TTFB)
- Resource timing breakdown

---

## Benchmarking Workflow

### Pre-Benchmark Checklist

- [ ] Clear browser cache
- [ ] Close unnecessary tabs
- [ ] Disable browser extensions
- [ ] Use incognito/private mode
- [ ] Ensure stable network connection
- [ ] Backend server running
- [ ] No other applications consuming resources

### Step-by-Step Benchmark Process

#### Step 1: Baseline Measurement

```bash
# Start development server
npm run dev

# Run Lighthouse
lighthouse http://localhost:5173 --output json --output-path baseline.json

# Record bundle size
npm run build
ls -lh build/
```

**Document Results:**
```markdown
## Baseline Results (Date: YYYY-MM-DD)

- Lighthouse Performance: XX/100
- Lighthouse Accessibility: XX/100
- Bundle Size (gzipped): XX KB
- Initial Load Time: X.XXs
- Time to Interactive: X.XXs
```

#### Step 2: Identify Bottlenecks

**Common Bottlenecks:**

1. **Large JavaScript Bundles**
   - Check `stats.html` for oversized chunks
   - Look for duplicate dependencies
   - Verify tree-shaking is working

2. **Slow API Responses**
   - Check Network tab for slow requests
   - Profile backend performance
   - Implement caching if needed

3. **Render Blocking Resources**
   - Identify CSS/JS blocking first paint
   - Defer non-critical scripts
   - Inline critical CSS

4. **Layout Shifts**
   - Check CLS score in Lighthouse
   - Ensure images have dimensions
   - Avoid dynamic content insertion

5. **Long Tasks**
   - Profile JavaScript execution
   - Break up long computations
   - Use Web Workers for heavy tasks

#### Step 3: Optimization Implementation

**Based on findings, implement optimizations:**

**Example: Code Splitting**
```typescript
// Before: Eager loading
import HeavyComponent from '$lib/components/HeavyComponent.svelte';

// After: Lazy loading
let HeavyComponent: any = null;
onMount(async () => {
  const module = await import('$lib/components/HeavyComponent.svelte');
  HeavyComponent = module.default;
});
```

**Example: Image Optimization**
```html
<!-- Before -->
<img src="large-image.png" />

<!-- After -->
<img 
  src="optimized-image.webp" 
  width="800" 
  height="600"
  loading="lazy"
  decoding="async"
/>
```

**Example: Debouncing**
```typescript
// Before: Immediate search on every keystroke
<input on:input={(e) => performSearch(e.target.value)} />

// After: Debounced search
const debouncedSearch = debounce(performSearch, 300);
<input on:input={(e) => debouncedSearch(e.target.value)} />
```

#### Step 4: Post-Optimization Measurement

```bash
# Rebuild after optimizations
npm run build

# Run Lighthouse again
lighthouse http://localhost:5173 --output json --output-path optimized.json

# Compare bundle sizes
du -sh build/
```

#### Step 5: Comparison & Documentation

**Create comparison report:**

```markdown
## Performance Improvement Report

### Before Optimization
- Performance Score: 75/100
- Bundle Size: 350KB (gzipped)
- LCP: 3.2s
- TTI: 4.5s

### After Optimization
- Performance Score: 92/100
- Bundle Size: 180KB (gzipped)
- LCP: 1.8s
- TTI: 2.3s

### Improvements
- ✅ Performance: +17 points
- ✅ Bundle Size: -48% reduction
- ✅ LCP: -44% faster
- ✅ TTI: -49% faster

### Changes Made
1. Implemented code splitting for 3 heavy components
2. Added lazy loading for route components
3. Optimized images (WebP format)
4. Debounced search input
5. Removed unused dependencies
```

---

## Automated Performance Testing

### CI/CD Integration

**GitHub Actions Example:**

```yaml
# .github/workflows/performance.yml
name: Performance Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  lighthouse:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '20'
      
      - name: Install dependencies
        run: npm ci
      
      - name: Build
        run: npm run build
      
      - name: Serve build directory
        run: npx serve -s build -l 3000 &
      
      - name: Run Lighthouse CI
        uses: treosh/lighthouse-ci-action@v9
        with:
          urls: |
            http://localhost:3000
          budgetPath: ./budget.json
          uploadArtifacts: true
      
      - name: Upload results
        uses: actions/upload-artifact@v3
        with:
          name: lighthouse-results
          path: .lighthouseci/
```

**Budget Configuration:**

```json
// budget.json
{
  "ci": {
    "collect": {
      "settings": {
        "preset": "desktop"
      }
    }
  },
  "budgets": [
    {
      "path": "/*",
      "resourceSizes": [
        {
          "resourceType": "script",
          "budget": 200
        },
        {
          "resourceType": "total",
          "budget": 500
        }
      ],
      "timings": [
        {
          "metric": "first-contentful-paint",
          "budget": 2000
        },
        {
          "metric": "interactive",
          "budget": 3500
        }
      ]
    }
  ]
}
```

---

## Monitoring in Production

### Real User Monitoring (RUM)

**Web Vitals Collection:**

```typescript
// src/lib/utils/metrics.ts
import { getCLS, getFID, getLCP, getFCP, getTTFB } from 'web-vitals';

function sendToAnalytics(metric: any) {
  // Send to your analytics endpoint
  fetch('/api/analytics', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: metric.name,
      value: metric.value,
      rating: metric.rating,
      navigationType: metric.navigationType
    })
  });
}

export function initPerformanceMonitoring() {
  getCLS(sendToAnalytics);
  getFID(sendToAnalytics);
  getLCP(sendToAnalytics);
  getFCP(sendToAnalytics);
  getTTFB(sendToAnalytics);
}
```

**Usage:**

```typescript
// src/app.d.ts or root layout
import { initPerformanceMonitoring } from '$lib/utils/metrics';

if (typeof window !== 'undefined') {
  initPerformanceMonitoring();
}
```

### Error Tracking Integration

**Sentry Setup:**

```bash
npm install @sentry/svelte @sentry/vite-plugin
```

```typescript
// src/hooks.server.ts
import * as Sentry from '@sentry/svelte';

Sentry.init({
  dsn: import.meta.env.VITE_SENTRY_DSN,
  environment: import.meta.env.MODE,
  tracesSampleRate: 0.1 // Sample 10% of transactions
});
```

---

## Performance Checklist

### Development Phase

- [ ] Enable TypeScript strict mode
- [ ] Use tree-shakeable imports
- [ ] Implement code splitting for heavy components
- [ ] Lazy load routes when possible
- [ ] Optimize images (WebP, proper sizing)
- [ ] Minimize re-renders (use `$derived`, `untrack`)
- [ ] Debounce/throttle expensive operations
- [ ] Use virtual scrolling for long lists
- [ ] Profile with Chrome DevTools regularly

### Build Phase

- [ ] Run bundle analyzer (`stats.html`)
- [ ] Verify tree-shaking effectiveness
- [ ] Check for duplicate dependencies
- [ ] Enable minification and compression
- [ ] Generate source maps for production debugging
- [ ] Test with production build locally

### Testing Phase

- [ ] Run Lighthouse audits (mobile & desktop)
- [ ] Test on real devices (low-end phones)
- [ ] Simulate slow networks (3G throttling)
- [ ] Measure Core Web Vitals
- [ ] Check accessibility scores
- [ ] Verify no console errors/warnings

### Deployment Phase

- [ ] Enable HTTP caching headers
- [ ] Configure CDN for static assets
- [ ] Enable gzip/brotli compression
- [ ] Set up performance monitoring
- [ ] Configure error tracking
- [ ] Document baseline metrics

### Ongoing Maintenance

- [ ] Monitor performance metrics weekly
- [ ] Review bundle size after each release
- [ ] Update dependencies regularly
- [ ] Profile new features before merging
- [ ] Collect real user metrics
- [ ] Address performance regressions immediately

---

## Common Performance Issues & Solutions

### Issue 1: Large Bundle Size

**Symptoms:**
- Slow initial load
- High memory usage
- Poor mobile performance

**Solutions:**
1. Run bundle analyzer to identify large dependencies
2. Replace heavy libraries with lighter alternatives
3. Implement code splitting
4. Remove unused dependencies
5. Use dynamic imports for optional features

### Issue 2: Slow API Responses

**Symptoms:**
- Loading spinners visible for long periods
- Timeout errors
- Poor user experience

**Solutions:**
1. Implement client-side caching
2. Add retry mechanisms with exponential backoff
3. Use optimistic updates where appropriate
4. Paginate large result sets
5. Compress API responses

### Issue 3: Janky Animations

**Symptoms:**
- Stuttering transitions
- Dropped frames
- Poor perceived performance

**Solutions:**
1. Use CSS transforms instead of position changes
2. Animate `opacity` and `transform` only
3. Use `will-change` sparingly
4. Reduce animation complexity on low-end devices
5. Respect `prefers-reduced-motion`

### Issue 4: Memory Leaks

**Symptoms:**
- Increasing memory usage over time
- Browser crashes on long sessions
- Sluggish performance after extended use

**Solutions:**
1. Clean up event listeners in `onDestroy`
2. Unsubscribe from stores when components unmount
3. Cancel pending async operations
4. Clear intervals and timeouts
5. Use Chrome DevTools Memory profiler

### Issue 5: Layout Shifts

**Symptoms:**
- Content jumps during loading
- Poor CLS score
- Frustrating user experience

**Solutions:**
1. Reserve space for dynamic content
2. Set explicit width/height on images
3. Use CSS aspect-ratio for media
4. Load fonts with `font-display: swap`
5. Avoid inserting content above existing content

---

## Performance Budget

Define and enforce performance budgets:

```json
{
  "performance-budget": {
    "initial-load": {
      "javascript": "200KB",
      "css": "50KB",
      "images": "500KB",
      "total": "1MB"
    },
    "per-page": {
      "javascript": "100KB",
      "requests": 50
    },
    "metrics": {
      "lcp": 2500,
      "fid": 100,
      "cls": 0.1,
      "tti": 3000
    }
  }
}
```

---

## Resources

### Tools
- **Lighthouse**: https://developer.chrome.com/docs/lighthouse/
- **WebPageTest**: https://www.webpagetest.org/
- **Bundle Analyzer**: https://github.com/btd/rollup-plugin-visualizer
- **Web Vitals**: https://web.dev/vitals/

### Guides
- **Web.dev Performance**: https://web.dev/performance/
- **Chrome DevTools**: https://developer.chrome.com/docs/devtools/
- **Svelte Performance**: https://svelte.dev/docs/performance

### Books
- "High Performance Browser Networking" by Ilya Grigorik
- "Using WebPageTest" by Rick Viscomi

---

**Last Updated**: 2026-05-02  
**Document Version**: 1.0

For questions or improvements, please contribute to the project repository.
