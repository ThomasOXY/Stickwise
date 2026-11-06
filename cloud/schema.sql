-- Stickwise Cloud: paste into phpMyAdmin (or `mysql <db> < schema.sql`) on an
-- empty database. MySQL 5.7+ or MariaDB 10.3+. All times are UTC.

SET NAMES utf8mb4;

-- Nobody can sign up: the only account is made once by install.php.
CREATE TABLE users (
  id            INT UNSIGNED NOT NULL AUTO_INCREMENT,
  username      VARCHAR(64)  NOT NULL,
  password_hash VARCHAR(255) NOT NULL,
  created_at    DATETIME     NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_users_username (username)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- What the app signs in with. Only the SHA-256 of a token is stored.
CREATE TABLE api_tokens (
  id           INT UNSIGNED NOT NULL AUTO_INCREMENT,
  user_id      INT UNSIGNED NOT NULL,
  token_hash   CHAR(64)     NOT NULL,
  label        VARCHAR(100) NOT NULL DEFAULT '',
  created_at   DATETIME     NOT NULL,
  last_used_at DATETIME     NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_api_tokens_hash (token_hash),
  KEY ix_api_tokens_user (user_id),
  CONSTRAINT fk_api_tokens_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- One file in storage/. `track_key` comes from the app: the hash of the source
-- audio and the conversion used, so a track shared twice is uploaded once.
CREATE TABLE tracks (
  id           INT UNSIGNED NOT NULL AUTO_INCREMENT,
  user_id      INT UNSIGNED NOT NULL,
  track_key    CHAR(64)     NOT NULL,
  file_name    VARCHAR(64)  NOT NULL,
  mime         VARCHAR(32)  NOT NULL,
  size_bytes   BIGINT UNSIGNED NOT NULL,
  title        VARCHAR(255) NOT NULL DEFAULT '',
  artist       VARCHAR(255) NOT NULL DEFAULT '',
  duration_s   INT UNSIGNED NOT NULL DEFAULT 0,
  created_at   DATETIME     NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_tracks_key (user_id, track_key),
  CONSTRAINT fk_tracks_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- `slug` is the random part of the link; `client_ref` is the playlist's id in
-- the app, so sharing it again keeps the link. No password hash = open link.
CREATE TABLE playlists (
  id            INT UNSIGNED NOT NULL AUTO_INCREMENT,
  user_id       INT UNSIGNED NOT NULL,
  client_ref    VARCHAR(64)  NOT NULL,
  slug          CHAR(32)     NOT NULL,
  title         VARCHAR(255) NOT NULL,
  password_hash VARCHAR(255) NULL,
  created_at    DATETIME     NOT NULL,
  updated_at    DATETIME     NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_playlists_slug (slug),
  UNIQUE KEY uq_playlists_ref (user_id, client_ref),
  CONSTRAINT fk_playlists_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE playlist_tracks (
  playlist_id INT UNSIGNED NOT NULL,
  position    INT UNSIGNED NOT NULL,
  track_id    INT UNSIGNED NOT NULL,
  PRIMARY KEY (playlist_id, position),
  KEY ix_playlist_tracks_track (track_id),
  CONSTRAINT fk_pt_playlist FOREIGN KEY (playlist_id) REFERENCES playlists (id) ON DELETE CASCADE,
  CONSTRAINT fk_pt_track    FOREIGN KEY (track_id)    REFERENCES tracks (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Failed password tries, to slow down guessing. `bucket` hashes the scope and IP.
CREATE TABLE failed_attempts (
  id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  bucket       CHAR(64) NOT NULL,
  attempted_at DATETIME NOT NULL,
  PRIMARY KEY (id),
  KEY ix_failed_attempts (bucket, attempted_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
