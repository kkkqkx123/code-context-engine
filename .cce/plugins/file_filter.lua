-- plugin/file_filter.lua
-- Demo: FileFilter — inclusion/exclusion decisions during scanning.
--
-- The scanner consults this plugin before the built-in PatternMatcher.
-- "exclude" force-skips a path, "include" force-includes it (even when the
-- built-in matcher would reject it), and "neutral" (nil) defers to built-in.
--
-- Requires `scanner.plugin_filter_enabled = true` in config.

plugin = {
    id = "file_filter_plugin",
    name = "File Filter",
    version = "0.1.0",
    priority = 10,
    description = "Force-excludes scratch dirs and force-includes a custom extension (FileFilter demo).",
    capabilities = { "file_filter" }
}

function plugin.filter_file(file_path, is_directory, size)
    -- Force-exclude scratch/backup paths regardless of built-in rules.
    if file_path:find("scratch", 1, true)
        or file_path:find("%.bak$")
        or file_path:find("~$")
    then
        return "exclude"
    end
    -- Force-include a non-standard config extension the built-in matcher
    -- would normally skip.
    if file_path:find("%.cconf$") then
        return "include"
    end
    return nil
end
