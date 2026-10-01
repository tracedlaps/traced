# Fallback for every locale: every key lives here.

## Shared
start = Start
stop = Stop
cancel = Cancel
open-settings = Open settings
try-again = Try again
finishing = Finishing…
stat-top = Top
stat-avg = Avg
stat-distance = Distance
stat-time = Time
pb = PB
no-completed-laps = No completed laps
sector-count = { $count ->
    [one] 1 sector
   *[other] { $count } sectors
}

## Tabs
tab-history = History
tab-tracks = Tracks
tab-speed = Speed
tab-editor = Editor
tab-settings = Settings
back-to-live = Recording · back to live

## GPS
gps-off = GPS off
gps-connecting = GPS connecting…
gps-no-fix = No GPS fix
gps-fix = GPS fix · { $source } · { $satellites ->
    [one] 1 satellite
   *[other] { $satellites } satellites
} · ±{ $accuracy }
gps-permission-denied = Location permission denied. It's needed to record.
gps-location-disabled = Location is turned off. Turn it on to record.
gps-unavailable = The GPS isn't available: { $detail }
gps-no-device = No GPS device found.
permission-ask = Location permission is asked for on Start
permission-granted = Location permission granted
permission-blocked = Location permission is blocked. Allow it in Settings, then Start again.

## Background guide
guide-title = Keep recording with the screen off
guide-why = Your phone's battery manager can close Traced while the screen is off, ending the recording. Allow it to run in the background:
guide-killed-title = Your last recording was cut short
guide-killed = Your phone's battery manager closed Traced while it was recording. Allow it to run in the background so it doesn't happen again:
guide-step-settings = Tap Open settings.
guide-step-battery = Open Battery usage (or Battery).
guide-step-allow = Choose Allow background activity (on some phones, Unrestricted).
guide-done = Done
guide-not-yet = Traced still isn't allowed to run in the background. Follow the steps, then tap Done.
guide-skip = Not now

## Tracks and speedometer
tracks = Tracks
no-tracks = No tracks yet.
import-a-track = Import a track
edit-track = Edit track
speedometer = Speedometer
speedometer-idle = { $unit } · top, average and distance, no track
developer = Developer
mock-drive = Mock drive instead of the GPS
force-location-manager = LocationManager instead of Fused

## Live
not-recording = Not recording.
new-recording = New recording
waiting-for-start = Waiting for the start line…
lap-n = Lap { $n }
sector-n = Sector { $n }
finished = Finished
last-sector = Last sector { $time }
last-lap = Last lap { $time }
best = Best { $time }
delta = Delta
gps-ended = GPS stream ended. Stop to see the results.
g-calibrating = Calibrating…

## Results and history
results = Results
recording-in-progress = Recording in progress.
personal-best = Personal best on this track
splits = Splits
gps-gap = GPS dropout during this lap
g-braking = Max braking
g-accelerating = Max acceleration
g-lateral = Max lateral (L · R)
g-lap = Brake · Lat g
history = History
no-recordings = No recordings yet.
recording-not-found = Recording not found.
delete-recording = Delete recording
keep-recording = Keep recording
delete-recording-warning = Deletes this recording for good.
delete-recording-failed = Couldn't delete the recording.
interrupted = Interrupted
interrupted-detail = Interrupted: the app closed mid-recording, kept up to its last fix.
no-track = No track
speedometer-only = Speedometer only
outcome-laps = { $count ->
    [one] 1 lap
   *[other] { $count } laps
} · best { $best }
outcome-run = Run { $best }

## Editor
editor = Editor
create-track = Create new track
record-track = Record a track
import-file = Import a CSV, GPX or KML file
edit-saved-track = Edit a saved track
track-not-found = Track not found.
recorded-track = Recorded track
record-hint = Drive or walk the track. Stop at the end, or back at the start for a loop.
starting-gps = Starting the GPS…
mode-draw = Draw
mode-gate = Gate
mode-start = Start
mode-finish = Finish
mode-sectors = Sectors
hint-selected = Drag it to move it, or delete it.
hint-draw = Tap the map to add waypoints. Drag to move one, or drag a hollow one to insert it. Tap one to delete it.
hint-sectors = Tap waypoints to add or remove sector gates.
hint-gate = Tap a waypoint to move the gate there.
reverse = Reverse
undo = Undo
delete-point = Delete point
name = Name
tags = Tags
tags-hint = Add a tag, e.g. karting
closed-loop = Closed loop, timed in laps
live-track-locked = Stop the recording on this track to move its gates or delete it.
save-track = Save track
discard = Discard
delete-track = Delete track
keep-track = Keep track
delete-warning = { $count ->
    [0] Deletes the track.
    [one] Deletes the track and its recording.
   *[other] Deletes the track and its { $count } recordings.
}
save-failed = Couldn't save the track.
delete-failed = Couldn't delete the track.

## Track errors
too-few-waypoints = Tap the map to add at least two waypoints.
placement-off-track = A gate is { $distance } from the track, too far to place on it.
placement-beyond-track = A gate is { $distance } past the end of the track. Extend the waypoints to cover it.
finish-before-start = The finish must come after the start along the waypoints.
sector-outside = A sector gate isn't between the start and the finish.
import-unrecognised = Not a CSV with latitude and longitude columns, a GPX or a KML file.
import-bad-line = Line { $line } has no valid latitude and longitude.
import-bad-xml = Not valid XML: { $detail }

## Map
finding-location = Finding your location…
location-not-found = Couldn't find your location.
no-places = No places found.
search-failed = Search failed: { $error }
search-place = Search for a place
layer-streets = Streets
layer-satellite = Satellite
my-location = My location
fit-track = Fit track

## Settings
settings = Settings
units = Units
units-metric = Metric
units-imperial = Imperial
theme = Theme
theme-system = System
theme-light = Light
theme-dark = Dark
language = Language
language-system = System
language-en = English
language-pt = Português
live-map = Live map
live-map-follow = Follow me
live-map-overview = Whole track

## Backup
backup = Backup
backup-note = Everything lives only on this phone. Export it to keep a copy.
export-backup = Export backup
restore-warning = Restoring replaces every track and recording here with the backup's.
restore-choose = Choose backup and replace
restore-backup = Restore backup
backup-saved = Backup saved.
backup-restored = Backup restored.
backup-failed = Backup failed: { $error }

## Updates
updates = Updates
update-version = Traced { $version }
update-checking = Checking for a newer version…
update-none = This is the latest version.
update-check-failed = Couldn't check for updates: { $error }
update-install = Install version { $version }
update-whats-new = What's new in { $version }
update-downloading = Downloading…
no-internet = No internet connection
update-failed = Update failed: { $error }
update-available = Version { $version } is available · update

back-to-replay = Replaying · back to replay
replay = Replay
end-replay = End replay
replay-ended = Replay finished.
replay-trim = Skip before the start and after the finish

## Share
share = Share
share-track = Share track
share-with-best = Show my best lap
share-with-gpx = Attach the track file (GPX)
save-image = Save image
save-gpx = Save track file (GPX)
share-saved = Saved.
share-failed = Couldn't share: { $error }
card-length = Length
card-sectors = Sectors
card-best-lap = Best lap
