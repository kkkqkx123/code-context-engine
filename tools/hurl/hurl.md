# Hurl Agent Ruleset (LLM Internal)

## 1. File Extension
- Always use `.hurl` for request files.

## 2. Request Block Syntax
```
METHOD URL
Headers*
[Body|Form|MultipartFormData|GraphQL|SOAP]
HTTP ExpectedStatus
[Captures]
[Asserts]
```

## 3. Captures
- Store data for reuse: `name: query`
- Supported queries:
  - `header "Name"`  
  - `cookie "Name"`  
  - `jsonpath "$.field"`  
  - `xpath "//tag/@attr"`  
  - `regex /pattern/` (first match)  
  - `duration` (milliseconds)  
  - `status` (integer)  
  - `body` (raw bytes)

## 4. Variables
- Reference with `{{var}}`
- Inject via CLI: `--variable var=value` or `--variables-file csv`
- Precedence: CLI > file > default

## 5. Assertions
- Operators: `==`, `!=`, `<`, `>`, `<=`, `>=`, `contains`, `startsWith`, `endsWith`, `exists`, `matches`, `count`, `includes`
- Targets: `status`, `header`, `cookie`, `jsonpath`, `xpath`, `body`, `sha256`, `duration`

## 6. Chaining
- Separate blocks with blank lines; variables cascade forward.

## 7. Options for Execution
- `--test` → enable all assertions, exit non-zero on failure
- `--parallel N` → run N workers
- `--report-junit file.xml` → CI-friendly output
- `--json` → structured stdout for parsing
- `--verbose` → full traffic dump
- `--error-format long` → include request/response on failure
- `--interactive` → pause between requests
- `--follow-redirect` → follow 3xx (default 50 max)
- `--insecure` → skip TLS verify
- `--max-time SEC` → global timeout
- `--connect-timeout SEC` → TCP handshake timeout
- `--retry N` → retry on failure
- `--delay MS` → fixed delay between requests
- `--variable` / `--variables-file` → inject variables

## 8. Body Types
- Raw: plain text or JSON inline
- `[Form]` → `key: value` (x-www-form-urlencoded)
- `[MultipartFormData]` → `name: file,@path;type=mime`
- `[Cookies]` → `name: value` (client jar)
- GraphQL: inline `{ "query": "..." }` or ```graphql block
- SOAP: full XML with proper `Content-Type`

## 9. Hashing & Bytes
- `sha256` / `sha512` / `md5` → compare hex digest  
  Example: `sha256 == hex,deadbeef...`

## 10. Performance Checks
- `duration < 500` (milliseconds)

## 11. Regex
- Use `matches` operator:  
  `jsonpath "$.id" matches /\d{4}/`

## 12. XPath
- Default namespace-aware; use `string()` to coerce to text.

## 13. JSONPath
- Dot and bracket notation both accepted; root is `$`.

## 14. Exit Codes
- `0` → all passed  
- `1` → assertion or runtime failure  
- `2` → parsing error

## 15. Stdout / Stderr
- Stdout: response body (last request) unless `--json` or `--output file`
- Stderr: logs and errors; keep separate for clean piping

## 16. Security
- Never embed secrets in files; always pass via variables or env:
  ```
  hurl --variable token=$ENV:TOKEN login.hurl
  ```

## 17. Template for New Task
```
GET https://{{host}}/api/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"
duration < 1000
```

## 18. Quick Convert from cURL
- Use `hurl --to-hurl` (experimental) or manual mapping:
  - `-X` → METHOD
  - `-H` → header line
  - `-d` → body
  - `-F` → `[MultipartFormData]`
  - `-L` → `--follow-redirect`
  - `-u user:pass` → `Authorization: Basic ...` (compute base64)

## 19. Limits
- Max redirect: 50 (change with `--max-redirs`)
- Max time: none by default; set with `--max-time`
- File size: streamed, memory footprint low

## 20. Reference URLs
- Spec: https://hurl.dev/docs/
- JSONPath: https://github.com/json-path/JsonPath
- XPath 1.0: https://www.w3.org/TR/xpath-10/
