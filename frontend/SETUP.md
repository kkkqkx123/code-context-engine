# Frontend Directory Structure

## Overview

The frontend project is now properly isolated in the `frontend/` directory to avoid polluting the root directory.

## Directory Structure

```
code-context-engine/          # Root directory (Rust backend)
├── cce-cli/                  # CLI application
├── src/                      # Backend source
├── Cargo.toml                # Rust dependencies
├── .gitignore                # Git ignore rules (includes node_modules)
└── frontend/                 # Frontend application (SvelteKit)
    ├── src/                  # Frontend source code
    ├── node_modules/         # NPM dependencies (NOT in git)
    ├── package.json          # NPM dependencies list
    ├── svelte.config.js      # SvelteKit configuration
    ├── vite.config.ts        # Vite build configuration
    ├── tsconfig.json         # TypeScript configuration
    ├── .gitignore            # Frontend-specific ignore rules
    └── README.md             # Frontend documentation
```

## Key Points

1. **Isolated Dependencies**: All Node.js dependencies (`node_modules/`) are inside `frontend/` directory
2. **No Root Pollution**: No `package.json` or `node_modules/` in the project root
3. **Independent Build**: Frontend can be built independently from the backend
4. **Git Safety**: `node_modules/` is excluded from version control by root `.gitignore`

## Commands

All frontend commands should be run from the `frontend/` directory:

```bash
# Navigate to frontend directory
cd frontend

# Install dependencies
npm install

# Development server (http://localhost:3001)
npm run dev

# Production build
npm run build

# Preview production build
npm run preview

# Type checking
npm run check
```

## Proxy Configuration

The frontend development server proxies API requests to the backend:
- Frontend: `http://localhost:3001`
- Backend API: `http://localhost:9000` (proxied via `/api` prefix)

This is configured in `vite.config.ts`.

## Migration Notes

If you previously had `node_modules/` in the root directory, it has been moved to `frontend/`. This ensures:
- Cleaner project structure
- No dependency conflicts between Rust and Node.js ecosystems
- Proper separation of concerns
