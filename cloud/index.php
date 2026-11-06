<?php
// The listening page: index.php?p=<slug>. No playlist list and no account
// pages here: without the exact link there is nothing to see.
declare(strict_types=1);

require __DIR__ . '/lib/bootstrap.php';

const PASSWORD_ATTEMPTS = 10;

function page_start(string $title): void
{
    send_page_headers();
    ?><!doctype html>
<html lang="cs">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="robots" content="noindex, nofollow">
<meta name="color-scheme" content="dark">
<title><?= e($title) ?></title>
<link rel="stylesheet" href="assets/style.css">
</head>
<?php
}

function notice_page(int $status, string $heading, string $text): never
{
    http_response_code($status);
    page_start($heading);
    ?><body class="centered">
<main class="card">
  <h1><?= e($heading) ?></h1>
  <p><?= e($text) ?></p>
</main>
</body>
</html>
<?php
    exit;
}

function duration_text(int $seconds): string
{
    return $seconds >= 3600
        ? sprintf('%d:%02d:%02d', intdiv($seconds, 3600), intdiv($seconds % 3600, 60), $seconds % 60)
        : sprintf('%d:%02d', intdiv($seconds, 60), $seconds % 60);
}

function tracks_word(int $n): string
{
    return match (true) {
        $n === 1 => '1 skladba',
        $n >= 2 && $n <= 4 => "$n skladby",
        default => "$n skladeb",
    };
}

$slug = (string) ($_GET['p'] ?? '');
try {
    $playlist = find_playlist($slug);
    if ($playlist === null) {
        notice_page(404, 'Tady nic není', 'Odkaz neplatí, nebo už playlist není sdílený.');
    }

    $wrong = false;
    $limited = false;
    if (!is_unlocked($playlist) && ($_SERVER['REQUEST_METHOD'] ?? '') === 'POST') {
        $scope = 'playlist:' . $playlist['id'];
        if (too_many_attempts($scope, PASSWORD_ATTEMPTS)) {
            $limited = true;
        } elseif (password_verify((string) ($_POST['password'] ?? ''), $playlist['password_hash'])) {
            clear_failed_attempts($scope);
            remember_unlocked($playlist);
            header('Location: index.php?p=' . $playlist['slug'], true, 303);
            exit;
        } else {
            note_failed_attempt($scope);
            $wrong = true;
        }
    }

    if (!is_unlocked($playlist)) {
        http_response_code($wrong ? 401 : ($limited ? 429 : 200));
        page_start($playlist['title']);
        ?><body class="centered">
<main class="card">
  <h1><?= e($playlist['title']) ?></h1>
  <form method="post" action="index.php?p=<?= e($playlist['slug']) ?>">
    <label for="password">Heslo</label>
    <input id="password" name="password" type="password" autocomplete="current-password" required autofocus>
    <?php if ($wrong): ?><p class="error" role="alert">Tohle heslo nesedí.</p><?php endif; ?>
    <?php if ($limited): ?><p class="error" role="alert">Moc pokusů. Zkus to znovu za pár minut.</p><?php endif; ?>
    <button type="submit">Otevřít</button>
  </form>
</main>
</body>
</html>
<?php
        exit;
    }

    $tracks = query(
        'SELECT t.id, t.title, t.artist, t.duration_s FROM playlist_tracks pt
         JOIN tracks t ON t.id = pt.track_id
         WHERE pt.playlist_id = ? ORDER BY pt.position',
        [$playlist['id']],
    )->fetchAll();
} catch (Throwable $err) {
    error_log('stickwise page: ' . $err);
    notice_page(500, 'Něco se pokazilo', 'Zkus stránku načíst znovu za chvíli.');
}

$total = array_sum(array_column($tracks, 'duration_s'));
page_start($playlist['title']);
?><body class="listen">
<header class="head">
  <h1><?= e($playlist['title']) ?></h1>
  <p class="meta"><?= e(tracks_word(count($tracks))) ?><?= $total ? ', ' . e(duration_text($total)) : '' ?></p>
</header>

<ol class="tracks" id="tracks">
<?php foreach ($tracks as $i => $t): ?>
  <li>
    <button type="button" class="track" data-src="stream.php?p=<?= e($playlist['slug']) ?>&amp;t=<?= (int) $t['id'] ?>" data-title="<?= e($t['title']) ?>" data-artist="<?= e($t['artist']) ?>">
      <span class="no"><?= $i + 1 ?></span>
      <span class="names"><span class="title"><?= e($t['title'] !== '' ? $t['title'] : 'Bez názvu') ?></span><span class="artist"><?= e($t['artist']) ?></span></span>
      <span class="time"><?= $t['duration_s'] ? e(duration_text((int) $t['duration_s'])) : '' ?></span>
    </button>
  </li>
<?php endforeach; ?>
</ol>

<footer class="player" id="player" hidden>
  <div class="now"><span class="title" id="now-title"></span><span class="artist" id="now-artist"></span></div>
  <div class="controls">
    <button type="button" id="prev" aria-label="Předchozí skladba"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 5h2.5v14H6zM20 5v14L9.5 12z"/></svg></button>
    <button type="button" id="toggle" class="main" aria-label="Přehrát"><svg viewBox="0 0 24 24" aria-hidden="true"><path id="toggle-icon" d="M7 4v16l14-8z"/></svg></button>
    <button type="button" id="next" aria-label="Další skladba"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15.5 5H18v14h-2.5zM4 5v14l10.5-7z"/></svg></button>
  </div>
  <div class="seek">
    <span id="elapsed">0:00</span>
    <input id="seek" type="range" min="0" max="1000" value="0" step="1" aria-label="Pozice ve skladbě">
    <span id="length">0:00</span>
  </div>
  <p class="error" id="play-error" role="alert" hidden>Skladbu se nepovedlo načíst.</p>
</footer>
<audio id="audio" preload="none"></audio>
<script src="assets/player.js"></script>
</body>
</html>
