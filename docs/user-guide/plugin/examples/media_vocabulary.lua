-- Media domain vocabulary (example plugin).
--
-- Provides the abbreviations of the ISO base media file format so that the
-- indexed text carries their meaning. The core engine ships no domain
-- terminology at all: enable this plugin only for media projects.
--
-- Register it in the project registry (`.cce/plugins.json`):
--
--   {
--     "plugins": [
--       {
--         "id": "media-vocabulary",
--         "path": "media_vocabulary.lua",
--         "enabled": true,
--         "plugin_type": "lua",
--         "languages": ["c", "cpp"]
--       }
--     ]
--   }

plugin = {
    id = "media-vocabulary",
    name = "Media Vocabulary",
    version = "1.0.0",
    description = "Domain terminology for ISO base media file format abbreviations",
    capabilities = { "vocabulary" },
}

-- Returns the terminology table. The host matches the tokens against whole
-- identifiers and renders the matched entries as a `Glossary:` line.
function plugin.vocabulary()
    return {
        { token = "ftyp", expansion = "file type box" },
        { token = "moov", expansion = "movie box" },
        { token = "mdat", expansion = "media data box" },
        { token = "mvhd", expansion = "movie header box" },
        { token = "tkhd", expansion = "track header box" },
        { token = "mdhd", expansion = "media header box" },
        { token = "hdlr", expansion = "handler reference box" },
        { token = "vmhd", expansion = "video media header box" },
        { token = "smhd", expansion = "sound media header box" },
        { token = "stbl", expansion = "sample table box" },
        { token = "stsd", expansion = "sample description box" },
        { token = "stts", expansion = "decoding time to sample box" },
        { token = "ctts", expansion = "composition time to sample box" },
        { token = "stsc", expansion = "sample to chunk box" },
        { token = "stsz", expansion = "sample size box" },
        { token = "stco", expansion = "chunk offset box" },
        { token = "co64", expansion = "64-bit chunk offset box" },
        { token = "stss", expansion = "sync sample box" },
        { token = "sdtp", expansion = "sample dependency type box" },
        { token = "sbgp", expansion = "sample to group box" },
        { token = "sgpd", expansion = "sample group description box" },
        { token = "subs", expansion = "sub-sample information box" },
        { token = "avc1", expansion = "AVC video track sample entry" },
        { token = "avcC", expansion = "AVC decoder configuration record" },
        { token = "avc3", expansion = "AVC video track sample entry with in-band parameter sets" },
        { token = "hvc1", expansion = "HEVC video track sample entry" },
        { token = "hvcC", expansion = "HEVC decoder configuration record" },
        { token = "mp4a", expansion = "MPEG-4 audio track sample entry" },
        { token = "esds", expansion = "MPEG-4 elementary stream descriptor" },
        { token = "btrt", expansion = "bitrate box" },
        { token = "pasp", expansion = "pixel aspect ratio box" },
        { token = "colr", expansion = "colour information box" },
        { token = "clap", expansion = "clean aperture box" },
        { token = "mvex", expansion = "movie extends box" },
        { token = "trex", expansion = "track extends box" },
        { token = "mehd", expansion = "movie extends header box" },
        { token = "mfhd", expansion = "movie fragment header box" },
        { token = "traf", expansion = "track fragment box" },
        { token = "tfhd", expansion = "track fragment header box" },
        { token = "tfdt", expansion = "track fragment base media decode time box" },
        { token = "trun", expansion = "track fragment run box" },
        { token = "sidx", expansion = "segment index box" },
        { token = "ssix", expansion = "subsegment index box" },
        { token = "sdp", expansion = "session description protocol" },
        { token = "rtp", expansion = "real-time transport protocol" },
        { token = "rtcp", expansion = "real-time transport control protocol" },
        { token = "rtsp", expansion = "real-time streaming protocol" },
        { token = "hls", expansion = "HTTP live streaming" },
        { token = "dash", expansion = "dynamic adaptive streaming over HTTP" },
    }
end
