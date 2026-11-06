<?php
// Makes the one account, once. Needs the `secret` from config.php, and refuses
// to do anything as soon as an account exists. Safe to delete afterwards.
declare(strict_types=1);

require __DIR__ . '/lib/bootstrap.php';

const INSTALL_ATTEMPTS = 8;
const MIN_PASSWORD = 10;

/** What to fix, by MySQL's error number. Public page: no hosts, names or the raw message. */
function db_problem(Throwable $err): string
{
    $code = $err instanceof PDOException ? (int) ($err->errorInfo[1] ?? $err->getCode()) : 0;
    return match ($code) {
        1045 => 'Server odmítl jméno nebo heslo (chyba 1045). Zkontroluj db_user a db_pass v config.php.',
        1044 => 'Uživatel nemá přístup k téhle databázi (chyba 1044). Zkontroluj db_name a práva uživatele.',
        1049 => 'Databáze s tímhle názvem neexistuje (chyba 1049). Zkontroluj db_name, hosting k názvu často přidává předponu.',
        1146 => 'V databázi chybí tabulky (chyba 1146). Vlož schema.sql přímo do databáze z db_name.',
        1142 => 'Uživatel nesmí číst tabulky (chyba 1142). Přidej mu práva k databázi.',
        2002, 2003, 2005, 2006 => "Na databázový server se nejde připojit (chyba $code). Zkontroluj db_host: hosting často nepoužívá localhost, ale vlastní adresu.",
        0 => 'Nepodařilo se připojit k databázi: ' . get_class($err) . '.',
        default => "Databáze hlásí chybu $code.",
    };
}

$checks = [];
$message = '';
$failed = false;
$done = false;
$accountExists = false;

$checks[] = ['PHP 8.0 nebo novější', PHP_VERSION_ID >= 80000, PHP_VERSION];
$checks[] = ['Rozšíření PDO MySQL', extension_loaded('pdo_mysql'), ''];
$checks[] = ['Rozšíření mbstring', extension_loaded('mbstring'), ''];

try {
    config();
    $checks[] = ['Soubor config.php', true, ''];
    try {
        $accountExists = (int) query('SELECT COUNT(*) FROM users')->fetchColumn() > 0;
        $checks[] = ['Databáze a tabulky', true, ''];
    } catch (Throwable $err) {
        error_log('stickwise install: ' . $err);
        $checks[] = ['Databáze a tabulky', false, db_problem($err)];
    }
    $checks[] = ['Zápis do složky s audiem', is_dir(storage_dir()) && is_writable(storage_dir()), ''];
} catch (RuntimeException $err) {
    $checks[] = ['Soubor config.php', false, $err->getMessage()];
}

$maxUpload = ini_bytes('post_max_size');
$checks[] = ['Největší nahrávaný soubor (post_max_size)', $maxUpload >= 32 * 1024 ** 2, round($maxUpload / 1024 ** 2) . ' MB, doporučeno aspoň 32 MB'];

$ready = !array_filter($checks, fn ($c) => !$c[1] && !str_starts_with($c[0], 'Největší'));

if ($ready && !$accountExists && ($_SERVER['REQUEST_METHOD'] ?? '') === 'POST') {
    $username = trim((string) ($_POST['username'] ?? ''));
    $password = (string) ($_POST['password'] ?? '');
    $failed = true;
    if (too_many_attempts('install', INSTALL_ATTEMPTS)) {
        $message = 'Moc pokusů. Zkus to znovu za pár minut.';
    } elseif (!hash_equals(config()['secret'], (string) ($_POST['secret'] ?? ''))) {
        note_failed_attempt('install');
        $message = 'Instalační klíč nesedí se „secret“ v config.php.';
    } elseif ($username === '' || mb_strlen($username) > 64) {
        $message = 'Zadej přihlašovací jméno.';
    } elseif (mb_strlen($password) < MIN_PASSWORD) {
        $message = 'Heslo musí mít aspoň ' . MIN_PASSWORD . ' znaků.';
    } elseif ($password !== (string) ($_POST['password2'] ?? '')) {
        $message = 'Hesla se neshodují.';
    } else {
        // The check and the insert in one statement: two forms sent at once make one account.
        $made = query(
            'INSERT INTO users (username, password_hash, created_at)
             SELECT ?, ?, UTC_TIMESTAMP() FROM DUAL
             WHERE NOT EXISTS (SELECT 1 FROM users)',
            [$username, password_hash($password, PASSWORD_DEFAULT)],
        )->rowCount();
        $failed = false;
        $done = $made === 1;
        $accountExists = true;
    }
}

send_page_headers();
?><!doctype html>
<html lang="cs">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<meta name="color-scheme" content="dark">
<title>Stickwise Cloud: instalace</title>
<link rel="stylesheet" href="assets/style.css">
</head>
<body class="centered">
<main class="card wide">
  <h1>Stickwise Cloud</h1>

  <?php if ($done): ?>
    <p class="ok" role="status">Účet je hotový. Přihlas se v appce Stickwise a tenhle soubor (install.php) můžeš smazat.</p>
  <?php elseif ($accountExists): ?>
    <p>Účet už existuje, další založit nejde. Tenhle soubor (install.php) můžeš smazat.</p>
  <?php else: ?>
    <ul class="checks">
      <?php foreach ($checks as [$label, $ok, $detail]): ?>
        <li class="<?= $ok ? 'pass' : 'miss' ?>">
          <span><?= e($label) ?></span>
          <?php if ($detail !== ''): ?><small><?= e($detail) ?></small><?php endif; ?>
        </li>
      <?php endforeach; ?>
    </ul>

    <?php if ($ready): ?>
      <form method="post" action="install.php">
        <label for="secret">Instalační klíč („secret“ z config.php)</label>
        <input id="secret" name="secret" type="password" autocomplete="off" required>
        <label for="username">Přihlašovací jméno</label>
        <input id="username" name="username" type="text" autocomplete="username" maxlength="64" required>
        <label for="password">Heslo (aspoň <?= MIN_PASSWORD ?> znaků)</label>
        <input id="password" name="password" type="password" autocomplete="new-password" minlength="<?= MIN_PASSWORD ?>" required>
        <label for="password2">Heslo znovu</label>
        <input id="password2" name="password2" type="password" autocomplete="new-password" required>
        <?php if ($failed): ?><p class="error" role="alert"><?= e($message) ?></p><?php endif; ?>
        <button type="submit">Založit účet</button>
      </form>
    <?php else: ?>
      <p class="error">Oprav označené položky a načti stránku znovu.</p>
    <?php endif; ?>
  <?php endif; ?>
</main>
</body>
</html>
