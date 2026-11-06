<?php
// The API the Stickwise app talks to. Everything but `ping` and `login` needs
// the token from `login` in the X-Stickwise-Token header (not Authorization:
// CGI and FastCGI hosting often drop that one).
declare(strict_types=1);

require __DIR__ . '/lib/bootstrap.php';

const LOGIN_ATTEMPTS = 8;
const MIMES = ['m4a' => 'audio/mp4', 'mp3' => 'audio/mpeg'];

function reply(array $data, int $status = 200): never
{
    http_response_code($status);
    header('Content-Type: application/json; charset=utf-8');
    header('Cache-Control: no-store');
    header('X-Robots-Tag: noindex');
    echo json_encode($data, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
    exit;
}

function fail(string $code, int $status): never
{
    reply(['error' => $code], $status);
}

function json_body(): array
{
    $data = json_decode((string) file_get_contents('php://input'), true);
    is_array($data) || fail('bad_request', 400);
    return $data;
}

function require_user(): int
{
    $token = $_SERVER['HTTP_X_STICKWISE_TOKEN'] ?? '';
    if ($token === '') {
        fail('unauthorized', 401);
    }
    $hash = hash('sha256', $token);
    $row = query('SELECT user_id FROM api_tokens WHERE token_hash = ?', [$hash])->fetch();
    if (!$row) {
        fail('unauthorized', 401);
    }
    query('UPDATE api_tokens SET last_used_at = UTC_TIMESTAMP() WHERE token_hash = ?', [$hash]);
    return (int) $row['user_id'];
}

function valid_key(mixed $key): bool
{
    return is_string($key) && preg_match('/^[a-f0-9]{64}$/', $key) === 1;
}

/** The app's playlist id as sent, or a 400. */
function client_ref(array $body): string
{
    $ref = $body['ref'] ?? null;
    if (!is_string($ref) || $ref === '' || strlen($ref) > 64) {
        fail('bad_request', 400);
    }
    return $ref;
}

function owned_playlist(int $user, string $ref): ?array
{
    return query('SELECT * FROM playlists WHERE user_id = ? AND client_ref = ?', [$user, $ref])->fetch() ?: null;
}

function playlist_reply(array $p): array
{
    $n = query('SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?', [$p['id']])->fetchColumn();
    return [
        'slug' => $p['slug'],
        'title' => $p['title'],
        'hasPassword' => $p['password_hash'] !== null,
        'tracks' => (int) $n,
        'updatedAt' => $p['updated_at'],
    ];
}

function hash_playlist_password(mixed $password): string
{
    if (!is_string($password) || $password === '' || strlen($password) > 200) {
        fail('bad_password', 400);
    }
    return password_hash($password, PASSWORD_DEFAULT);
}

/**
 * Remove audio nothing plays any more: what `$removed` lost now, and what an
 * upload left behind over a day ago (not newer: it may be about to be used).
 */
function sweep_tracks(int $user, array $removed): void
{
    $ids = array_map('intval', $removed);
    $mine = $ids ? 't.id IN (' . implode(',', $ids) . ') OR ' : '';
    $orphans = query(
        "SELECT t.id, t.file_name FROM tracks t
         LEFT JOIN playlist_tracks pt ON pt.track_id = t.id
         WHERE t.user_id = ? AND pt.track_id IS NULL
           AND ($mine t.created_at < UTC_TIMESTAMP() - INTERVAL 1 DAY)",
        [$user],
    )->fetchAll();
    foreach ($orphans as $t) {
        query('DELETE FROM tracks WHERE id = ?', [$t['id']]);
        @unlink(storage_dir() . '/' . $t['file_name']);
    }
}

function action_ping(): never
{
    reply(['app' => 'stickwise-cloud', 'version' => CLOUD_VERSION, 'maxUpload' => ini_bytes('post_max_size')]);
}

function action_login(): never
{
    $body = json_body();
    if (too_many_attempts('login', LOGIN_ATTEMPTS)) {
        fail('too_many_attempts', 429);
    }
    $user = query('SELECT id, password_hash FROM users WHERE username = ?', [(string) ($body['username'] ?? '')])->fetch();
    if (!$user || !password_verify((string) ($body['password'] ?? ''), $user['password_hash'])) {
        note_failed_attempt('login');
        fail('bad_login', 401);
    }
    clear_failed_attempts('login');
    $token = bin2hex(random_bytes(32));
    query(
        'INSERT INTO api_tokens (user_id, token_hash, label, created_at) VALUES (?, ?, ?, UTC_TIMESTAMP())',
        [$user['id'], hash('sha256', $token), substr((string) ($body['label'] ?? ''), 0, 100)],
    );
    reply(['token' => $token]);
}

function action_logout(): never
{
    require_user();
    query('DELETE FROM api_tokens WHERE token_hash = ?', [hash('sha256', $_SERVER['HTTP_X_STICKWISE_TOKEN'])]);
    reply(['ok' => true]);
}

/** Of the keys sent, those whose audio the server does not have. */
function action_missing(): never
{
    $user = require_user();
    $missing = [];
    foreach (json_body()['keys'] ?? [] as $key) {
        valid_key($key) || fail('bad_request', 400);
        $file = query('SELECT file_name FROM tracks WHERE user_id = ? AND track_key = ?', [$user, $key])->fetchColumn();
        if ($file === false || !is_file(storage_dir() . '/' . $file)) {
            $missing[] = $key;
        }
    }
    reply(['missing' => array_values(array_unique($missing))]);
}

/** The body is the audio file itself. Checked against `sha256` before it counts. */
function action_upload(): never
{
    $user = require_user();
    $key = $_GET['key'] ?? null;
    $format = $_GET['format'] ?? null;
    $sha256 = $_GET['sha256'] ?? null;
    if (!valid_key($key) || !valid_key($sha256) || !is_string($format) || !isset(MIMES[$format])) {
        fail('bad_request', 400);
    }
    // PHP passes on no body at all when it is over post_max_size.
    if ((int) ($_SERVER['CONTENT_LENGTH'] ?? 0) > ini_bytes('post_max_size')) {
        fail('too_large', 413);
    }
    @set_time_limit(0);

    $dir = storage_dir();
    $name = bin2hex(random_bytes(16)) . '.' . $format;
    $tmp = "$dir/$name.part";
    $in = fopen('php://input', 'rb');
    $out = @fopen($tmp, 'wb');
    if ($in === false || $out === false) {
        fail('storage_not_writable', 500);
    }
    $written = stream_copy_to_stream($in, $out);
    fclose($in);
    fclose($out);
    if ($written === false || $written === 0 || !hash_equals($sha256, hash_file('sha256', $tmp))) {
        @unlink($tmp);
        fail('upload_damaged', 422);
    }
    rename($tmp, "$dir/$name");

    // A row can exist with its file gone (deleted over FTP): the new file takes over.
    $old = query('SELECT file_name FROM tracks WHERE user_id = ? AND track_key = ?', [$user, $key])->fetchColumn();
    query(
        'INSERT INTO tracks (user_id, track_key, file_name, mime, size_bytes, created_at)
         VALUES (?, ?, ?, ?, ?, UTC_TIMESTAMP())
         ON DUPLICATE KEY UPDATE file_name = VALUES(file_name), mime = VALUES(mime), size_bytes = VALUES(size_bytes)',
        [$user, $key, $name, MIMES[$format], $written],
    );
    if ($old !== false) {
        @unlink("$dir/$old");
    }
    reply(['ok' => true]);
}

function action_playlist_get(): never
{
    $user = require_user();
    $p = owned_playlist($user, client_ref(json_body()));
    reply(['playlist' => $p ? playlist_reply($p) : null]);
}

/**
 * Publish the playlist in one go, after its audio is uploaded: until then the
 * link keeps playing what it did. `password` left out keeps the current one.
 */
function action_playlist_save(): never
{
    $user = require_user();
    $body = json_body();
    $ref = client_ref($body);
    $title = $body['title'] ?? null;
    $tracks = $body['tracks'] ?? null;
    if (!is_string($title) || trim($title) === '' || !is_array($tracks) || !$tracks) {
        fail('bad_request', 400);
    }
    $hash = array_key_exists('password', $body) ? hash_playlist_password($body['password']) : null;

    $pdo = db();
    $pdo->beginTransaction();
    $find = $pdo->prepare('SELECT id FROM tracks WHERE user_id = ? AND track_key = ?');
    $meta = $pdo->prepare('UPDATE tracks SET title = ?, artist = ?, duration_s = ? WHERE id = ?');
    $ids = [];
    foreach ($tracks as $t) {
        valid_key($t['key'] ?? null) || fail('bad_request', 400);
        $find->execute([$user, $t['key']]);
        $id = $find->fetchColumn();
        if ($id === false) {
            $pdo->rollBack();
            fail('track_missing', 409);
        }
        $meta->execute([
            mb_substr((string) ($t['title'] ?? ''), 0, 255),
            mb_substr((string) ($t['artist'] ?? ''), 0, 255),
            max(0, (int) ($t['duration'] ?? 0)),
            $id,
        ]);
        $ids[] = (int) $id;
    }

    $p = owned_playlist($user, $ref);
    if ($p) {
        $sql = 'UPDATE playlists SET title = ?, updated_at = UTC_TIMESTAMP()' . ($hash !== null ? ', password_hash = ?' : '') . ' WHERE id = ?';
        query($sql, $hash !== null ? [$title, $hash, $p['id']] : [$title, $p['id']]);
    } else {
        query(
            'INSERT INTO playlists (user_id, client_ref, slug, title, password_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, UTC_TIMESTAMP(), UTC_TIMESTAMP())',
            [$user, $ref, bin2hex(random_bytes(16)), $title, $hash],
        );
    }
    $p = owned_playlist($user, $ref);
    $before = query('SELECT track_id FROM playlist_tracks WHERE playlist_id = ?', [$p['id']])->fetchAll(PDO::FETCH_COLUMN);
    query('DELETE FROM playlist_tracks WHERE playlist_id = ?', [$p['id']]);
    $add = $pdo->prepare('INSERT INTO playlist_tracks (playlist_id, position, track_id) VALUES (?, ?, ?)');
    foreach ($ids as $position => $id) {
        $add->execute([$p['id'], $position, $id]);
    }
    $pdo->commit();

    sweep_tracks($user, $before);
    reply(['playlist' => playlist_reply($p)]);
}

/** `password` null removes it. Everyone who entered the old one has to enter the new one. */
function action_playlist_password(): never
{
    $user = require_user();
    $body = json_body();
    $p = owned_playlist($user, client_ref($body)) ?? fail('not_found', 404);
    $hash = ($body['password'] ?? null) === null ? null : hash_playlist_password($body['password']);
    query('UPDATE playlists SET password_hash = ?, updated_at = UTC_TIMESTAMP() WHERE id = ?', [$hash, $p['id']]);
    reply(['playlist' => playlist_reply(owned_playlist($user, $p['client_ref']))]);
}

function action_playlist_delete(): never
{
    $user = require_user();
    $p = owned_playlist($user, client_ref(json_body())) ?? fail('not_found', 404);
    $before = query('SELECT track_id FROM playlist_tracks WHERE playlist_id = ?', [$p['id']])->fetchAll(PDO::FETCH_COLUMN);
    query('DELETE FROM playlists WHERE id = ?', [$p['id']]);
    sweep_tracks($user, $before);
    reply(['ok' => true]);
}

$actions = [
    'ping' => 'action_ping',
    'login' => 'action_login',
    'logout' => 'action_logout',
    'missing' => 'action_missing',
    'upload' => 'action_upload',
    'playlist_get' => 'action_playlist_get',
    'playlist_save' => 'action_playlist_save',
    'playlist_password' => 'action_playlist_password',
    'playlist_delete' => 'action_playlist_delete',
];

$action = $actions[$_GET['action'] ?? ''] ?? null;
if ($action === null) {
    fail('not_found', 404);
}
if ($action !== 'action_ping' && ($_SERVER['REQUEST_METHOD'] ?? '') !== 'POST') {
    fail('method_not_allowed', 405);
}

try {
    $action();
} catch (Throwable $err) {
    error_log('stickwise api: ' . $err);
    fail('server_error', 500);
}
