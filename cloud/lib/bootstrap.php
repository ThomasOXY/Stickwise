<?php
// Shared by every page: config, database, attempt limits, listener cookies.
declare(strict_types=1);

const CLOUD_VERSION = 1;
const REMEMBER_DAYS = 30;
const ATTEMPT_WINDOW_MINUTES = 15;

// Errors go to the server's log, never into a page or an audio stream.
ini_set('display_errors', '0');

function config(): array
{
    static $config = null;
    if ($config === null) {
        $path = __DIR__ . '/../config.php';
        if (!is_file($path)) {
            throw new RuntimeException('config.php is missing');
        }
        $config = require $path;
        if (strlen($config['secret'] ?? '') < 32) {
            throw new RuntimeException('secret in config.php is shorter than 32 characters');
        }
    }
    return $config;
}

function db(): PDO
{
    static $pdo = null;
    if ($pdo === null) {
        $c = config();
        $pdo = new PDO(
            "mysql:host={$c['db_host']};dbname={$c['db_name']};charset=utf8mb4",
            $c['db_user'],
            $c['db_pass'],
            [
                PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
                PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC,
                PDO::ATTR_EMULATE_PREPARES => false,
            ],
        );
        $pdo->exec("SET time_zone = '+00:00'");
    }
    return $pdo;
}

function query(string $sql, array $params = []): PDOStatement
{
    $st = db()->prepare($sql);
    $st->execute($params);
    return $st;
}

function storage_dir(): string
{
    return rtrim(config()['storage'] ?? (__DIR__ . '/../storage'), '/');
}

function ini_bytes(string $key): int
{
    $v = trim((string) ini_get($key));
    $n = (int) $v;
    return match (strtolower(substr($v, -1))) {
        'g' => $n * 1024 ** 3,
        'm' => $n * 1024 ** 2,
        'k' => $n * 1024,
        default => $n,
    };
}

function attempt_bucket(string $scope): string
{
    return hash('sha256', $scope . '|' . ($_SERVER['REMOTE_ADDR'] ?? ''));
}

/** True when this visitor failed `$max` times for `$scope` within the window. */
function too_many_attempts(string $scope, int $max): bool
{
    query('DELETE FROM failed_attempts WHERE attempted_at < UTC_TIMESTAMP() - INTERVAL ' . ATTEMPT_WINDOW_MINUTES . ' MINUTE');
    $n = query('SELECT COUNT(*) FROM failed_attempts WHERE bucket = ?', [attempt_bucket($scope)])->fetchColumn();
    return (int) $n >= $max;
}

function note_failed_attempt(string $scope): void
{
    query('INSERT INTO failed_attempts (bucket, attempted_at) VALUES (?, UTC_TIMESTAMP())', [attempt_bucket($scope)]);
}

function clear_failed_attempts(string $scope): void
{
    query('DELETE FROM failed_attempts WHERE bucket = ?', [attempt_bucket($scope)]);
}

function find_playlist(string $slug): ?array
{
    if (!preg_match('/^[a-f0-9]{32}$/', $slug)) {
        return null;
    }
    return query('SELECT * FROM playlists WHERE slug = ?', [$slug])->fetch() ?: null;
}

/** The listener's proof of the password. Tied to its hash: a new password ends it. */
function unlock_signature(array $playlist, int $expires): string
{
    return hash_hmac('sha256', "{$playlist['id']}|$expires|{$playlist['password_hash']}", config()['secret']);
}

function unlock_cookie_name(array $playlist): string
{
    return 'sw_' . substr($playlist['slug'], 0, 12);
}

function is_https(): bool
{
    return ($_SERVER['HTTPS'] ?? 'off') !== 'off'
        || ($_SERVER['SERVER_PORT'] ?? '') === '443';
}

function is_unlocked(array $playlist): bool
{
    if ($playlist['password_hash'] === null) {
        return true;
    }
    [$expires, $signature] = array_pad(explode('.', $_COOKIE[unlock_cookie_name($playlist)] ?? '', 2), 2, '');
    return ctype_digit($expires)
        && (int) $expires > time()
        && hash_equals(unlock_signature($playlist, (int) $expires), $signature);
}

function remember_unlocked(array $playlist): void
{
    $expires = time() + REMEMBER_DAYS * 86400;
    setcookie(unlock_cookie_name($playlist), $expires . '.' . unlock_signature($playlist, $expires), [
        'expires' => $expires,
        'path' => dirname($_SERVER['SCRIPT_NAME']),
        'secure' => is_https(),
        'httponly' => true,
        'samesite' => 'Lax',
    ]);
}

function e(string $s): string
{
    return htmlspecialchars($s, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8');
}

function send_page_headers(): void
{
    header('Content-Type: text/html; charset=utf-8');
    header('X-Robots-Tag: noindex, nofollow');
    header('Referrer-Policy: no-referrer');
    header('X-Content-Type-Options: nosniff');
    header('Cache-Control: private, no-store');
    header("Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; media-src 'self'; img-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'");
}
