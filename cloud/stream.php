<?php
// Plays one track of a playlist, with HTTP Range so the player can seek. Audio
// reaches a listener only through here, after the playlist's password check.
declare(strict_types=1);

require __DIR__ . '/lib/bootstrap.php';

function stop(int $status): never
{
    http_response_code($status);
    exit;
}

try {
    $playlist = find_playlist((string) ($_GET['p'] ?? '')) ?? stop(404);
    is_unlocked($playlist) || stop(403);
    $track = query(
        'SELECT t.file_name, t.mime FROM tracks t
         JOIN playlist_tracks pt ON pt.track_id = t.id
         WHERE pt.playlist_id = ? AND t.id = ? LIMIT 1',
        [$playlist['id'], (int) ($_GET['t'] ?? 0)],
    )->fetch() ?: stop(404);
} catch (Throwable $err) {
    error_log('stickwise stream: ' . $err);
    stop(500);
}

$path = storage_dir() . '/' . $track['file_name'];
if (!is_file($path)) {
    stop(404);
}
$size = filesize($path);
$start = 0;
$end = $size - 1;
$partial = false;

// One range only; a list of ranges gets the whole file, which HTTP allows.
if (preg_match('/^bytes=(\d*)-(\d*)$/', $_SERVER['HTTP_RANGE'] ?? '', $m) && ($m[1] !== '' || $m[2] !== '')) {
    if ($m[1] === '') {
        $start = max(0, $size - (int) $m[2]);
    } else {
        $start = (int) $m[1];
        if ($m[2] !== '') {
            $end = min($end, (int) $m[2]);
        }
    }
    if ($start > $end || $start >= $size) {
        header("Content-Range: bytes */$size");
        stop(416);
    }
    $partial = true;
}

ini_set('zlib.output_compression', '0');
while (ob_get_level() > 0) {
    ob_end_clean();
}
@set_time_limit(0);

http_response_code($partial ? 206 : 200);
header('Content-Type: ' . $track['mime']);
header('Content-Disposition: inline');
header('Accept-Ranges: bytes');
header('Content-Length: ' . ($end - $start + 1));
header('Cache-Control: private, no-transform');
header('X-Content-Type-Options: nosniff');
header('X-Robots-Tag: noindex, nofollow');
if ($partial) {
    header("Content-Range: bytes $start-$end/$size");
}
if (($_SERVER['REQUEST_METHOD'] ?? '') === 'HEAD') {
    exit;
}

$file = fopen($path, 'rb');
fseek($file, $start);
$left = $end - $start + 1;
while ($left > 0 && !feof($file) && !connection_aborted()) {
    $chunk = fread($file, min(256 * 1024, $left));
    if ($chunk === false || $chunk === '') {
        break;
    }
    echo $chunk;
    flush();
    $left -= strlen($chunk);
}
fclose($file);
