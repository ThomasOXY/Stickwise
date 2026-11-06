<?php
// Copy to config.php and fill in. Never commit or share config.php.
return [
    'db_host' => 'localhost',
    'db_name' => 'stickwise',
    'db_user' => 'stickwise',
    'db_pass' => '',

    // A long random text, at least 32 characters (`openssl rand -hex 32`).
    // It signs the listeners' cookies and guards install.php. Changing it
    // makes every listener enter the playlist password again.
    'secret' => '',

    // Where the audio files live. Best outside the web root if the hosting
    // allows it, e.g. '/home/you/stickwise-audio'. Default: the storage folder here.
    'storage' => __DIR__ . '/storage',
];
