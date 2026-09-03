# Toast Notification System - Quick Reference

## Overview

The toast notification system provides user feedback for actions across the application. Toasts appear in the top-right corner on desktop and bottom-center on mobile.

---

## Import

```typescript
import { toastActions } from '$lib/stores/toast';
```

---

## API Methods

### Basic Usage

```typescript
// Show a toast with default settings (info type, 5s duration)
toastActions.show('Message here');

// Show with specific type
toastActions.show('Saved successfully', 'success');
toastActions.show('Error occurred', 'error');
toastActions.show('Warning message', 'warning');
toastActions.show('Information', 'info');

// Custom duration (in milliseconds)
toastActions.show('Brief message', 'info', 2000);
```

### Convenience Methods

```typescript
// Success toast (green border)
toastActions.success('Operation completed');

// Error toast (red border, 10s default)
toastActions.error('Failed to save changes');

// Warning toast (yellow border)
toastActions.warning('Please review your input');

// Info toast (blue border)
toastActions.info('Processing your request');
```

### Manual Dismissal

```typescript
// Dismiss a specific toast by ID
const id = 'toast-123';
toastActions.dismiss(id);
```

---

## Toast Types & Styling

| Type | Color | Default Duration | Use Case |
|------|-------|------------------|----------|
| `success` | Green (#00c853) | 5s | Successful operations |
| `error` | Red (#ff3d00) | 10s | Errors and failures |
| `warning` | Yellow (#ffd600) | 5s | Warnings and cautions |
| `info` | Blue (#2196f3) | 5s | Informational messages |

---

## Examples by Module

### Storage Module

```typescript
// After clearing storage
try {
  await storageActions.clearSelected();
  toastActions.success('Storage cleared successfully');
} catch (error) {
  toastActions.error('Failed to clear storage');
}
```

### Watch Module

```typescript
// When starting watch
async function handleStartWatch() {
  try {
    await watchActions.startWatch(path, extensions, debounce);
    toastActions.success('File watcher started');
  } catch (error) {
    toastActions.error(error.message);
  }
}

// When stopping watch
async function handleStopWatch() {
  try {
    await watchActions.stopWatch();
    toastActions.success('File watcher stopped');
  } catch (error) {
    toastActions.error('Failed to stop watcher');
  }
}
```

### Tools Module

```typescript
// Code compression
async function handleCompress() {
  compressLoading = true;
  try {
    compressResult = await toolsApi.compress({ code, language });
    toastActions.success(`Code compressed: ${compressResult.reduction_percentage.toFixed(1)}% reduction`);
  } catch (error) {
    toastActions.error('Compression failed');
  } finally {
    compressLoading = false;
  }
}
```

---

## Best Practices

### ✅ Do

1. **Use appropriate types:**
   ```typescript
   // Good
   toastActions.success('File uploaded');
   toastActions.error('Upload failed');
   ```

2. **Keep messages concise:**
   ```typescript
   // Good
   toastActions.success('Saved');
   
   // Too verbose
   toastActions.success('Your changes have been successfully saved to the database');
   ```

3. **Provide context for errors:**
   ```typescript
   // Good
   toastActions.error(`Failed to connect: ${error.message}`);
   ```

4. **Use longer durations for important messages:**
   ```typescript
   toastActions.warning('This action cannot be undone', 8000);
   ```

### ❌ Don't

1. **Don't show toasts for every minor action:**
   ```typescript
   // Bad - too noisy
   on:click={() => toastActions.info('Button clicked')}
   ```

2. **Don't use error toasts for validation:**
   ```typescript
   // Bad - use inline form validation instead
   if (!email) toastActions.error('Email is required');
   
   // Good - show error next to input field
   ```

3. **Don't stack too many toasts:**
   - Limit to 2-3 concurrent toasts maximum
   - Consider grouping related messages

---

## Accessibility

The toast system is fully accessible:

- ✅ `aria-live="polite"` region announces toasts to screen readers
- ✅ `role="alert"` on individual toasts
- ✅ Keyboard accessible dismiss buttons
- ✅ Touch-friendly 44px minimum touch targets
- ✅ High contrast colors meet WCAG AA standards

---

## Responsive Behavior

### Desktop (>768px)
- Position: Top-right corner
- Animation: Slide in from right
- Max width: 400px

### Mobile (≤768px)
- Position: Bottom center
- Animation: Slide up from bottom
- Width: Full width with 1rem margins

---

## Customization

To customize toast behavior, edit:
- Store: `src/lib/stores/toast.ts`
- Component: `src/lib/components/ui/ToastContainer.svelte`
- Styles: Within ToastContainer.svelte `<style>` block

---

## Troubleshooting

### Toasts not appearing?
1. Check that `<ToastContainer />` is in `+layout.svelte`
2. Verify import path: `'$lib/stores/toast'`
3. Check browser console for errors

### Toasts not dismissing automatically?
- Ensure `duration` parameter is > 0
- Default duration is 5000ms (5 seconds)
- Error toasts default to 10000ms (10 seconds)

### Multiple toasts stacking?
- This is normal behavior
- Maximum visible toasts controlled by CSS z-index
- Old toasts auto-dismiss based on duration

---

## Future Enhancements

Potential improvements for future iterations:

1. **Toast queue management:**
   - Limit maximum concurrent toasts
   - Queue excess toasts

2. **Action buttons:**
   - Add undo/redo actions to toasts
   - Link toasts to relevant pages

3. **Persistent toasts:**
   - Option for manual dismiss only
   - Important announcements

4. **Toast positioning options:**
   - Allow custom positions per toast
   - Center alignment option

---

**Last Updated:** 2026-05-02  
**Version:** 1.0.0
