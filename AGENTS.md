# Agent Guidelines for code-search-helper

## Build & Test Commands

```bash
# Build
npm run build

# Test
npm test <relative-path>    # Run tests for a specific file or directory
# never run full test. always run specific test file or directory

# Type Check
npm run typecheck
or `tsc --noEmit`

# Development
npm run dev                 # Run backend dev server (port 3010)
cd frontend && npm run dev  # Run frontend dev server (port 3011)
```

## Architecture

**Core Stack**: TypeScript, Express.js, Inversify (IoC), Tree-sitter, Qdrant (vector DB), Nebula Graph (graph DB)

**Key Modules**:
- `src/api/` - Express routes and controllers
- `src/service/` - Business logic (indexing, searching, parsing)
- `src/database/` - Database integrations (Qdrant, Nebula)
- `src/embedders/` - Multiple embedding providers (OpenAI, Ollama, Gemini, Mistral, etc.)
- `src/mcp/` - Model Context Protocol implementation
- `src/utils/` - Helper functions, caching, logging
- `frontend/` - Independent TypeScript/Vite frontend (no framework)

**Core services**:

## Code Style & Conventions

- **Language**: TypeScript with strict mode enabled
- **Imports**: ES6 `import`/`export` syntax; use relative paths with proper extensions
- **Naming**: PascalCase for classes (e.g., `LoggerService`), camelCase for methods/functions, SCREAMING_SNAKE_CASE for constants
- **Decorators**: Use `@injectable()` from Inversify for DI; use JSDoc comments for documentation
- **Error Handling**: Avoid `console.log()`; use `LoggerService` for logging; handle async errors properly
- **No unused variables**: ESLint enforces this; fix with `npm run lint:fix`
- **Testing**: Jest with patterns `**/?(*.)+(test).ts` or `__tests__/**/*.test.ts`; avoid running full test suite without reason
- **Env Variables**: Use `process.env` with fallbacks; define in `.env` file. 