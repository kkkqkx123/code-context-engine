# Code Context Engine - User Manual

## Table of Contents

1. [Introduction](#introduction)
2. [Dashboard Overview](#1-dashboard-overview)
3. [Index Management](#2-index-management)
4. [Search Interface](#3-search-interface)
5. [Entity Explorer](#4-entity-explorer)
6. [Storage Management](#5-storage-management)
7. [File Watching](#6-file-watching)
8. [Developer Tools](#7-developer-tools)
9. [Summary Generation](#8-summary-generation)
10. [Configuration](#9-configuration)
11. [Troubleshooting](#troubleshooting)
12. [FAQ](#faq)

---

## Introduction

The Code Context Engine (CCE) Frontend is a web-based interface for managing, searching, and exploring codebases. It provides powerful features for code indexing, semantic search, entity relationship visualization, and automated documentation generation.

This manual covers all 9 modules of the application with detailed workflows and examples.

---

## 1. Dashboard Overview

### Purpose
The dashboard serves as the central hub for monitoring your indexed projects and system status.

### Key Features
- **Project Summary**: View total indexed projects, files, and entities
- **System Health**: Monitor backend connection status
- **Quick Actions**: Access frequently used features
- **Recent Activity**: Track recent indexing and search operations

### How to Use
1. Navigate to `/` (root path)
2. Review project statistics at a glance
3. Click on any module link in the navigation bar to access specific features

### Tips
- Bookmark the dashboard for quick access
- Check system status before performing large operations
- Use the dashboard to verify successful indexing operations

---

## 2. Index Management

### Purpose
Manage codebase indexing operations including adding new projects, viewing index status, and reindexing.

### Key Features
- **Add Project**: Index a new codebase directory
- **Project List**: View all indexed projects with metadata
- **Index Status**: Monitor indexing progress and completion
- **Reindex**: Update existing indexes when code changes

### Workflow: Adding a New Project

1. **Navigate to Index Module**
   - Click "Index" in the navigation bar
   - Or go to `/index`

2. **Add Project**
   - Click "Add Project" button
   - Enter project details:
     - **Path**: Absolute or relative path to codebase directory
     - **Name**: Display name for the project
     - **Language Filter** (optional): Specify languages to index
     - **Exclude Patterns** (optional): Files/directories to skip

3. **Monitor Progress**
   - Watch the progress indicator during indexing
   - View real-time file processing updates
   - Wait for completion confirmation

4. **Verify Index**
   - Check project appears in project list
   - Verify file count matches expectations
   - Test search functionality

### Workflow: Reindexing a Project

1. Select project from the list
2. Click "Reindex" button
3. Confirm the operation
4. Monitor progress until completion

### Best Practices
- Exclude `node_modules`, `.git`, `build/`, and other generated directories
- Index only relevant source code directories
- Use language filters for multi-language projects
- Schedule reindexing during off-peak hours for large projects

---

## 3. Search Interface

### Purpose
Perform semantic and keyword searches across indexed codebases.

### Key Features
- **Semantic Search**: Find code by meaning, not just keywords
- **Keyword Search**: Traditional text-based search
- **Filter by Language**: Narrow results to specific programming languages
- **Filter by Entity Type**: Search functions, classes, variables, etc.
- **Result Preview**: View code snippets in context

### Workflow: Performing a Search

1. **Navigate to Search**
   - Click "Search" in navigation
   - Or go to `/search`

2. **Enter Query**
   - Type your search query in the search box
   - Examples:
     - `"authentication middleware"` (semantic)
     - `"getUserProfile"` (keyword)
     - `"error handling in database"` (natural language)

3. **Apply Filters** (Optional)
   - Select language(s) from dropdown
   - Choose entity type(s)
   - Set relevance threshold

4. **Review Results**
   - Browse ranked results
   - Click on any result to view full entity details
   - Use pagination for large result sets

5. **Refine Search**
   - Modify query based on initial results
   - Adjust filters to narrow/widen scope
   - Save useful queries for future use

### Search Tips
- Use natural language for semantic search
- Be specific for better results
- Combine keywords with context descriptions
- Use quotes for exact phrase matching
- Leverage filters to reduce noise

### Advanced Search Techniques

**Finding Similar Code Patterns:**
```
Query: "function that validates email addresses"
Filter: Language = TypeScript, Type = Function
```

**Locating API Endpoints:**
```
Query: "HTTP POST endpoint for user registration"
Filter: Type = Route Handler
```

**Understanding Dependencies:**
```
Query: "imports React hooks"
Filter: Type = Import Statement
```

---

## 4. Entity Explorer

### Purpose
Explore individual code entities (functions, classes, variables) with detailed information and relationships.

### Key Features
- **Entity Details**: View complete entity metadata
- **Source Code**: See original code with syntax highlighting
- **Relationships**: Explore dependencies, callers, and callees
- **Call Graph**: Visualize function call chains
- **Inheritance Tree**: View class hierarchies
- **Cross-References**: Find all references to an entity

### Workflow: Exploring an Entity

1. **Access Entity**
   - From search results: Click on a result
   - Direct URL: `/entities/[entity-id]`
   - From relationship graphs: Click on node

2. **Review Entity Information**
   - **Name**: Entity identifier
   - **Type**: Function, class, variable, etc.
   - **Location**: File path and line numbers
   - **Language**: Programming language
   - **Documentation**: Doc comments if available

3. **View Source Code**
   - Scroll through code snippet
   - Syntax highlighting for readability
   - Line numbers for reference

4. **Explore Relationships**
   - **Callers**: Functions that call this entity
   - **Callees**: Functions called by this entity
   - **Dependencies**: Imported modules/packages
   - **References**: Where this entity is used

5. **Visualize Connections**
   - Click "View Call Graph" for interactive visualization
   - Use "Inheritance Tree" for class hierarchies
   - Zoom and pan in graph views
   - Click nodes to navigate to related entities

### Understanding Entity Types

- **Function/Method**: Executable code blocks
- **Class/Interface**: Type definitions
- **Variable/Constant**: Data storage
- **Import/Export**: Module dependencies
- **Route Handler**: API endpoints
- **Component**: UI components (React, Vue, Svelte)

### Navigation Tips
- Use breadcrumbs to track your exploration path
- Browser back button works for entity navigation
- Open entities in new tabs for comparison
- Bookmark important entities for quick access

---

## 5. Storage Management

### Purpose
Monitor and manage storage resources used by indexed projects.

### Key Features
- **Storage Statistics**: View disk usage breakdown
- **Language Distribution**: See storage by programming language
- **Clear Operations**: Remove indexes and free space
- **Cache Management**: Control cached data

### Workflow: Viewing Storage Status

1. **Navigate to Storage**
   - Click "Storage" in navigation
   - Or go to `/storage`

2. **Review Statistics**
   - Total storage used
   - Number of indexed projects
   - Breakdown by language
   - Vector database size
   - SQLite database size

3. **Analyze Usage**
   - Identify largest projects
   - Check language distribution
   - Monitor growth over time

### Workflow: Clearing Storage

⚠️ **Warning**: This operation is irreversible!

1. **Initiate Clear**
   - Click "Clear All Data" button
   - Read the warning dialog carefully

2. **Confirm Operation**
   - Type confirmation text if required
   - Click "Confirm" to proceed
   - Wait for completion

3. **Verify Cleanup**
   - Check storage statistics show zero usage
   - Confirm project list is empty
   - System ready for fresh indexing

### Best Practices
- Regularly monitor storage growth
- Remove unused project indexes
- Archive important data before clearing
- Plan storage capacity for large codebases

---

## 6. File Watching

### Purpose
Automatically detect and index file changes in watched directories.

### Key Features
- **Start/Stop Watching**: Control file monitoring
- **Event Feed**: Real-time file change notifications
- **Auto-Index**: Automatically reindex changed files
- **Pause/Resume**: Temporarily suspend watching
- **Configuration**: Customize watch behavior

### Workflow: Setting Up File Watching

1. **Navigate to Watch Module**
   - Click "Watch" in navigation
   - Or go to `/watch`

2. **Configure Watch Settings**
   - **Directory Path**: Directory to monitor
   - **Extensions**: File types to watch (e.g., `.ts`, `.js`, `.py`)
   - **Exclude Patterns**: Files/directories to ignore
   - **Debounce Delay**: Wait time before processing changes

3. **Start Watching**
   - Click "Start Watching" button
   - Verify status shows "Active"
   - Monitor event feed for initial scan

4. **Monitor Changes**
   - Watch real-time event log
   - See file modifications, additions, deletions
   - Observe automatic reindexing triggers

5. **Pause/Stop When Needed**
   - Click "Pause" to temporarily suspend
   - Click "Stop" to end watching session
   - Resume later with "Start" button

### Event Feed Controls
- **Pause Feed**: Stop scrolling to examine events
- **Clear Feed**: Remove old events from view
- **Auto-scroll**: Toggle automatic scrolling to latest events

### Use Cases

**Active Development:**
- Watch your main project directory
- Get immediate feedback on indexing
- Ensure search stays up-to-date

**Code Reviews:**
- Watch feature branches
- Track changes during review process
- Verify impact on existing code

**Learning Codebases:**
- Watch unfamiliar repositories
- Observe file structure and organization
- Understand development patterns

### Troubleshooting
- If no events appear, verify directory path is correct
- Check file extensions match your project's languages
- Increase debounce delay if too many events trigger
- Exclude build output directories to reduce noise

---

## 7. Developer Tools

### Purpose
Access specialized tools for code analysis, compression, and diagnostics.

### Key Features
- **Symbol Lookup**: Find symbols across projects
- **Code Compression**: Compress code for efficient storage
- **Dependency Analysis**: Analyze project dependencies
- **Diagnostic Tools**: Debug indexing and search issues

### Tool 1: Symbol Lookup

**Purpose**: Find symbol definitions and usages across all indexed projects.

**Workflow**:
1. Navigate to `/tools`
2. Select "Symbol Lookup" tab
3. Enter symbol name (e.g., `getUserData`)
4. Optionally filter by language
5. Click "Search"
6. Review results showing:
   - Symbol definitions
   - Usage locations
   - File paths and line numbers

**Use Cases**:
- Find where a function is defined
- Locate all usages of a variable
- Discover API endpoints by name
- Trace symbol references across modules

### Tool 2: Code Compression

**Purpose**: Compress code entities for efficient vector storage.

**Workflow**:
1. Navigate to `/tools`
2. Select "Compression" tab
3. Choose compression language
4. Select compression level
5. Click "Compress"
6. View compression statistics:
   - Original size
   - Compressed size
   - Compression ratio
   - Time taken

**Benefits**:
- Reduced storage requirements
- Faster vector database operations
- Improved search performance

### Tool 3: Diagnostics

**Purpose**: Debug and troubleshoot CCE operations.

**Available Diagnostics**:
- **Index Health**: Check index integrity
- **Search Performance**: Measure query response times
- **Database Status**: Verify database connections
- **Backend Connectivity**: Test API communication

**Workflow**:
1. Navigate to `/tools`
2. Select "Diagnostics" tab
3. Choose diagnostic test
4. Click "Run Test"
5. Review results and recommendations

### Best Practices
- Use symbol lookup before refactoring
- Run compression after major indexing operations
- Perform diagnostics when experiencing issues
- Document tool outputs for team knowledge sharing

---

## 8. Summary Generation

### Purpose
Generate natural language summaries of code entities for improved understanding and documentation.

### Key Features
- **Entity Summarization**: Convert code to natural language
- **Batch Processing**: Summarize multiple entities
- **Custom Templates**: Configure summary format
- **Export Options**: Save summaries for documentation

### Workflow: Generating Summaries

1. **Navigate to Summary Module**
   - Click "Summary" in navigation
   - Or go to `/summary`

2. **Select Entities**
   - Choose specific entities to summarize
   - Or select entire project
   - Filter by entity type if needed

3. **Configure Summary Options**
   - **Detail Level**: Brief, standard, or detailed
   - **Include Examples**: Add code examples
   - **Format**: Plain text, Markdown, or HTML
   - **Language**: Output language (if supported)

4. **Generate Summaries**
   - Click "Generate" button
   - Monitor progress for batch operations
   - Wait for completion

5. **Review and Export**
   - Read generated summaries
   - Edit if necessary
   - Export to file or copy to clipboard
   - Integrate into documentation

### Summary Quality Tips
- Provide clear entity names for better summaries
- Include doc comments in source code
- Use consistent coding conventions
- Review AI-generated summaries for accuracy

### Use Cases

**Documentation Generation**:
- Create API documentation
- Generate README sections
- Build knowledge base articles

**Code Review Assistance**:
- Understand unfamiliar code quickly
- Identify purpose of complex functions
- Explain architectural decisions

**Onboarding New Developers**:
- Provide overview of codebase structure
- Explain key components and modules
- Accelerate learning curve

---

## 9. Configuration

### Purpose
Customize CCE frontend behavior and appearance.

### Key Features
- **Theme Settings**: Light/dark mode preferences
- **API Configuration**: Backend connection settings
- **Display Options**: Customize UI elements
- **Performance Tuning**: Adjust caching and loading

### Workflow: Accessing Configuration

1. **Navigate to Config**
   - Click "Config" in footer
   - Or go to `/config`

2. **Review Current Settings**
   - View all configurable options
   - Check current values
   - Understand available choices

3. **Modify Settings**
   - Change desired options
   - Preview changes if applicable
   - Save configuration

4. **Verify Changes**
   - Refresh page if needed
   - Confirm settings persist
   - Test affected features

### Common Configuration Tasks

**Change API Endpoint**:
```
Setting: API Base URL
Value: http://localhost:9000 (default)
Use case: Connect to remote backend
```

**Adjust Page Size**:
```
Setting: Results Per Page
Value: 10, 25, 50, 100
Use case: Optimize for screen size and preference
```

**Enable/Disable Features**:
```
Setting: Feature Flags
Options: Enable/disable experimental features
Use case: Test new functionality safely
```

### Configuration Best Practices
- Document custom configurations for team
- Backup configuration before major changes
- Test changes in development environment first
- Use environment variables for sensitive settings

---

## Troubleshooting

### Common Issues and Solutions

#### Issue: Cannot Connect to Backend

**Symptoms**:
- Error messages about API connection
- Features not loading
- Timeout errors

**Solutions**:
1. Verify backend server is running
2. Check API URL in configuration
3. Ensure network connectivity
4. Check firewall/proxy settings
5. Review browser console for errors

#### Issue: Search Returns No Results

**Symptoms**:
- Empty search results
- "No matches found" message

**Solutions**:
1. Verify projects are indexed
2. Check search query spelling
3. Try broader search terms
4. Remove filters temporarily
5. Reindex if code has changed significantly

#### Issue: Slow Performance

**Symptoms**:
- Long load times
- Laggy interactions
- Timeout errors

**Solutions**:
1. Check network connection speed
2. Reduce page size in settings
3. Clear browser cache
4. Close unnecessary browser tabs
5. Contact administrator about server load

#### Issue: File Watching Not Detecting Changes

**Symptoms**:
- No events in event feed
- Stale search results

**Solutions**:
1. Verify watch directory path is correct
2. Check file extension filters
3. Review exclude patterns
4. Restart file watching
5. Check file permissions

#### Issue: Mobile Display Problems

**Symptoms**:
- Content overflow
- Unreadable text
- Broken layout

**Solutions**:
1. Refresh page
2. Clear mobile browser cache
3. Try different mobile browser
4. Report issue with device details
5. Use desktop until fixed

### Getting Help

If you encounter issues not covered here:

1. **Check Documentation**
   - Review GETTING_STARTED.md
   - Read module-specific sections above
   - Search FAQ section

2. **Browser Console**
   - Open developer tools (F12)
   - Check Console tab for errors
   - Copy error messages for support

3. **Network Tab**
   - Inspect API requests
   - Check response status codes
   - Verify request/response data

4. **Contact Support**
   - Provide detailed description
   - Include error messages
   - Share steps to reproduce
   - Mention browser and OS version

---

## FAQ

### General Questions

**Q: What programming languages are supported?**  
A: CCE supports all languages with tree-sitter parsers, including JavaScript, TypeScript, Python, Rust, Go, Java, C/C++, and more.

**Q: How much disk space does indexing require?**  
A: Depends on codebase size. Typically 10-30% of source code size for indexes and vectors.

**Q: Can I index multiple projects?**  
A: Yes, CCE supports unlimited projects. Each project maintains separate indexes.

**Q: Is my code sent to external services?**  
A: No, all processing happens locally. Your code never leaves your machine unless you configure remote backends.

### Performance Questions

**Q: How long does indexing take?**  
A: Varies by project size. Small projects (<1000 files): seconds. Large projects (>10,000 files): minutes to hours.

**Q: Can I pause and resume indexing?**  
A: Currently, indexing runs to completion. Plan indexing during off-peak hours for large projects.

**Q: Why is search slow?**  
A: First searches may be slower due to cold starts. Subsequent searches benefit from caching.

### Feature Questions

**Q: How accurate is semantic search?**  
A: Accuracy depends on code quality and query specificity. Natural language queries work best with well-documented code.

**Q: Can I export search results?**  
A: Currently, results display in-browser. Export features are planned for future releases.

**Q: Does CCE support private repositories?**  
A: Yes, CCE works with any local directory. Authentication is handled by your version control system.

### Technical Questions

**Q: What database does CCE use?**  
A: CCE uses SQLite for relational data and Qdrant for vector storage.

**Q: Can I run CCE on a server?**  
A: Yes, CCE supports server deployment with Docker. See deployment documentation.

**Q: How do I backup my indexes?**  
A: Copy the `.cce` directory in your project root. Restore by placing it back in the same location.

**Q: Is there an API for programmatic access?**  
A: Yes, CCE provides a REST API. See backend API documentation for details.

### Accessibility Questions

**Q: Is CCE accessible with screen readers?**  
A: Yes, CCE follows WCAG 2.1 AA guidelines and is tested with NVDA and VoiceOver.

**Q: Can I use CCE with keyboard only?**  
A: Absolutely. All features are accessible via keyboard navigation (Tab, Enter, Escape, Arrow keys).

**Q: Does CCE support high contrast mode?**  
A: Yes, CCE respects system high contrast settings and provides sufficient color contrast ratios.

---

## Appendix

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Navigate between interactive elements |
| `Enter` | Activate focused element |
| `Escape` | Close dialogs/menus |
| `/` | Focus search box (when implemented) |
| `?` | Show help (when implemented) |

### Glossary

- **Entity**: A code element (function, class, variable, etc.)
- **Index**: Processed representation of code for searching
- **Vector**: Numerical representation of code semantics
- **Semantic Search**: Search by meaning rather than exact text
- **Call Graph**: Visualization of function call relationships
- **Tree-sitter**: Parser generator tool used for code parsing

### Version History

- **v0.1.0** (Current): Initial release with 9 core modules
- Future versions will add collaboration features, advanced analytics, and enhanced visualization

---

**Last Updated**: 2026-05-02  
**Document Version**: 1.0  
**Application Version**: v0.1.0

For updates and improvements to this manual, please contribute to the project repository.
