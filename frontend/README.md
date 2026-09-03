# Code Context Engine - Frontend

Web-based interface for the Code Context Engine project, built with SvelteKit.

## Design System

This frontend follows the **Swiss Minimalist / Tech Industrial** design style:
- High contrast black-and-white color scheme
- Space Grotesk + Space Mono typography
- Sharp borders, no rounded corners
- Grid-based layouts
- Fast, mechanical animations

## Getting Started

### Prerequisites

- Node.js 18+
- npm or yarn

### Installation

```bash
npm install
```

### Development

```bash
npm run dev
```

The development server will start at `http://localhost:3001` and proxy API requests to the backend at `http://localhost:9000`.

### Build

```bash
npm run build
```

### Preview Production Build

```bash
npm run preview
```

### Type Checking

```bash
npm run check
```

## Project Structure

```
frontend/
├── src/
│   ├── lib/
│   │   ├── api/          # API client modules
│   │   ├── components/   # Reusable UI components
│   │   ├── stores/       # Svelte state stores
│   │   └── utils/        # Utility functions
│   ├── routes/           # Page routes (SvelteKit file-based routing)
│   ├── app.css           # Global styles and design tokens
│   └── app.html          # HTML template
├── static/               # Static assets
├── svelte.config.js      # SvelteKit configuration
├── vite.config.ts        # Vite configuration
└── package.json
```

## Features

- **Dashboard**: System health monitoring and metrics
- **Index Management**: Project lifecycle and indexing control
- **Search Interface**: Multi-modal code search (Vector, BM25, Hybrid)
- **Entity Explorer**: Browse functions, classes, and relationships
- **Storage Management**: Monitor and clean up indexes
- **File Watching**: Real-time file system monitoring
- **Developer Tools**: Code compression, diagnosis, symbol lookup
- **Configuration**: TOML config editor and hot reload

## API Configuration

By default, the frontend connects to the backend at `http://localhost:9000`. To change this:

1. Create a `.env.local` file
2. Add `VITE_API_BASE_URL=http://your-backend-url`

## Technology Stack

- **Framework**: SvelteKit with Svelte 5
- **Build Tool**: Vite
- **Styling**: Pure CSS (no frameworks)
- **State Management**: Svelte stores
- **HTTP Client**: Native Fetch API
