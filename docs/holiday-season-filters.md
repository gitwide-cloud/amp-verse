# Holiday and season music filters

In Library → Smart Playlists → Analysis and track filters → Add filter, choose **Holiday** or **Season**, then use **Choose values**. Multiple choices within a filter mean OR; separate filter rows mean AND, as with the existing filters. An empty text choice means any known value for that field; unclassified music does not match.

Holiday presets include Christmas, Halloween, Thanksgiving, New Year, Valentine’s Day, Easter, Hanukkah, Kwanzaa, Diwali, Ramadan, Eid, St Patrick’s Day and Independence Day. Season presets are Spring, Summer, Autumn and Winter.

These describe music content, not a scheduled date. No source audio, folders or embedded metadata are changed. Holiday recognition uses explicit saved `holiday` values, embedded `Holiday` tags and whole-word holiday labels in saved genres, albums or track titles. Season recognition uses saved `season`/embedded `Season` tags, season genre names, and clearly season-labelled collections (for example Summer Hits). Fall normalizes to Autumn. Seasons are not inferred from holidays, artist names, track titles, geography or the current date. Generic religious music or songs without reliable labels are not guessed into holiday categories.

Filters survive ordinary settings save/reload, export/import and playlist synchronization using the same portable preparation document. Desktop/server and Android implement the same classification. Older clients do not support these new field names: update participating clients/server together before syncing a playlist containing them. This does not automatically retag a library or add calendar scheduling.
