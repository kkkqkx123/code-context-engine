# Getting Started with Code Context Engine Frontend

Welcome to the Code Context Engine (CCE) Frontend! This guide will help you set up and start using the web interface for exploring and managing your codebase index.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Installation](#installation)
3. [Configuration](#configuration)
4. [Running the Application](#running-the-application)
5. [First-Time Setup](#first-time-setup)
6. [Next Steps](#next-steps)

---

## Prerequisites

Before you begin, ensure you have the following installed:

### Required Software

- **Node.js** (version 20.x or higher)
  - Download from: https://nodejs.org/
  - Verify installation: `node --version`
  
- **npm** (comes with Node.js)
  - Verify installation: `npm --version`

- **Code Context Engine Backend**
  - The backend server must be running on port 9000 (default)
  - See backend documentation for setup instructions

### Recommended Tools

- Modern web browser (Chrome, Firefox, Safari, or Edge)
- Code editor (VS Code recommended)
- Git for version control

---

## Installation

### Step 1: Clone the Repository

```bash
git clone <repository-url>
cd code-context-engine/frontend
```

### Step 2: Install Dependencies

```bash
npm install
```

This will install all required packages including:
- SvelteKit framework
- TypeScript
- Vite build tool
- UI components and utilities

### Step 3: Verify Installation

```bash
npm run check
```

This command checks for TypeScript errors and ensures everything is set up correctly.

---

## Configuration

### Environment Variables

Create a `.env.local` file in the `frontend` directory:

```env
# API Base URL (backend server)
VITE_API_BASE_URL=http://localhost:9000
```

**Available Environment Variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `VITE_API_BASE_URL` | `http://localhost:9000` | Backend API endpoint |

### Proxy Configuration

The development server includes a proxy configuration in `vite.config.ts` that forwards `/api` requests to the backend server. No additional configuration is needed for local development.

---

## Running the Application

### Development Mode

Start the development server with hot module replacement:

```bash
npm run dev
```

The application will be available at: **http://localhost:3001**

**Features:**
- Hot reload on file changes
- Detailed error messages
- Source maps for debugging

### Production Build

To create an optimized production build:

```bash
npm run build
```

The built files will be in the `build/` directory.

To preview the production build locally:

```bash
npm run preview
```

---

## First-Time Setup

### 1. Access the Dashboard

Open your browser and navigate to `http://localhost:3001`. You should see the CCE dashboard.

### 2. Verify Backend Connection

The dashboard will display the backend connection status. If you see connection errors:

- Ensure the backend server is running (`cargo run` in the backend directory)
- Check that the backend is listening on port 9000
- Verify the `VITE_API_BASE_URL` environment variable

### 3. Add Your First Project

Navigate to the **Index Management** page:

1. Click "Add Project" button
2. Enter the path to your codebase directory
3. Select programming languages to index
4. Click "Start Indexing"

The indexing process will:
- Scan all files in the directory
- Parse code using tree-sitter
- Extract entities (functions, classes, etc.)
- Generate embeddings for semantic search
- Build relationship indexes

**Note:** Initial indexing may take several minutes depending on project size.

### 4. Explore Your Codebase

Once indexing is complete, you can:

- **Search**: Use natural language queries to find relevant code
- **Browse Entities**: View functions, classes, and their relationships
- **View Call Graphs**: Visualize function call chains
- **Explore Inheritance**: See class hierarchies

---

## Module Overview

The frontend consists of 9 main modules:

### 1. Dashboard
Central hub showing system status, recent activity, and quick actions.

### 2. Index Management
Add, remove, and manage indexed projects. Monitor indexing progress.

### 3. Search Interface
Powerful search with filters for:
- Semantic search (vector-based)
- Keyword search (BM25)
- Entity type filtering
- Language filtering

### 4. Entity Explorer
Detailed view of code entities with:
- Function/class details
- Call graphs
- Inheritance trees
- Call chains

### 5. Storage Management
Monitor storage usage, clear indexes, and manage cached data.

### 6. File Watching
Set up automatic re-indexing when files change:
- Configure watched directories
- Set file extensions to monitor
- View real-time event feed

### 7. Tools
Developer utilities:
- **Code Compression**: Reduce token count for LLM context
- **Code Diagnosis**: Analyze code quality
- **Symbol Lookup**: Find symbols across projects

### 8. Configuration
System settings and preferences.

### 9. Summary Generation
Generate natural language summaries of code entities.

---

## Troubleshooting

### Common Issues

#### Backend Connection Failed

**Symptoms:** Error messages about API connection

**Solutions:**
1. Verify backend is running: `curl http://localhost:9000/api/health`
2. Check backend logs for errors
3. Ensure firewall isn't blocking port 9000

#### Indexing Stuck

**Symptoms:** Progress bar not moving

**Solutions:**
1. Check backend logs for parsing errors
2. Verify the directory path is correct
3. Ensure files are readable (permissions)
4. Try clearing the index and restarting

#### Search Returns No Results

**Solutions:**
1. Verify indexing completed successfully
2. Check if the query matches indexed content
3. Try simpler search terms
4. Verify entity extraction worked (check storage stats)

#### Slow Performance

**Solutions:**
1. Clear browser cache
2. Reduce number of concurrent watchers
3. Limit indexed file types
4. Check system resources (RAM, CPU)

### Getting Help

- **Documentation:** Check the `docs/` directory for detailed guides
- **Issues:** Report bugs on GitHub
- **Community:** Join our Discord/Slack channel

---

## Next Steps

Now that you're set up, explore these resources:

1. **[User Manual](USER_MANUAL.md)** - Detailed feature documentation
2. **[Architecture Guide](ARCHITECTURE.md)** - Understanding the codebase
3. **[API Reference](API_REFERENCE.md)** - Backend API documentation
4. **[Contributing Guide](CONTRIBUTING.md)** - How to contribute

### Quick Actions

- 📊 **View Dashboard**: Understand system status
- 🔍 **Try Searching**: Test semantic search capabilities
- 📁 **Add Projects**: Index your codebases
- ⚙️ **Configure Watchers**: Set up automatic updates

---

## System Requirements

### Minimum
- Node.js 20.x
- 2GB RAM
- Modern browser (last 2 versions)

### Recommended
- Node.js 20.x LTS
- 4GB+ RAM
- SSD storage for faster indexing
- Chrome/Firefox for best performance

---

**Last Updated:** 2026-05-02  
**Version:** 0.1.0

For questions or issues, please refer to the full documentation or contact the development team.
