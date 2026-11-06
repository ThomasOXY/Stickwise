# Stickwise Cloud

Serverová část sdílení playlistů k poslechu. Běží na obyčejném PHP hostingu s MySQL:
appka Stickwise sem nahraje převedené skladby a ty pošleš odkaz. Registrace neexistuje,
nahrávat může jen jediný účet.

Potřebuje PHP 8.0+ (`pdo_mysql`, `mbstring`), MySQL 5.7+ nebo MariaDB 10.3+ a **HTTPS**.
Přes nešifrované `http://` se appka nepřipojí, jinak by heslo šlo sítí čitelně.

## Instalace

1. **Databáze:** založ prázdnou databázi a vlož do ní `schema.sql` (třeba v phpMyAdminu).
2. **Konfigurace:** zkopíruj `config.sample.php` jako `config.php` a vyplň přístup k databázi.
   Do `secret` dej dlouhý náhodný text, vygeneruješ ho třeba příkazem `openssl rand -hex 32`.
3. **Nahrání:** přes FTP nahraj celou složku na web, třeba do `/sdileni`. Skryté soubory
   `.htaccess` nahraj taky, chrání složky `lib` a `storage`.
4. **Práva:** PHP musí moct zapisovat do složky `storage`.
5. **Limity PHP:** nastav `post_max_size` aspoň na `64M` a `max_input_time` na `300`.
   `upload_max_filesize` se tu neuplatní, appka neposílá formulář. Web server může mít vlastní
   limit velikosti požadavku (`LimitRequestBody` u Apache, `client_max_body_size` u nginx).
6. **Účet:** otevři `https://tvuj-web/sdileni/install.php`, zadej `secret` a založ si účet.
   Druhý účet už založit nejde. Soubor `install.php` pak smaž.

## Používání

V appce klikni pravým tlačítkem na playlist a vyber **Sdílet k poslechu…**. Poprvé se
přihlásíš adresou webu (`https://tvuj-web/sdileni`), jménem a heslem. Appka si drží jen
token, heslo neukládá.

- **Převod:** skladby se převedou na AAC 192 kb/s, asi 9 MB na 6 minut. MP3 a M4A do 192 kb/s se
  posílají tak, jak jsou.
- **Aktualizace:** odkaz zůstává stejný a nahrají se jen změny. Než je audio celé nahrané,
  odkaz dál hraje původní verzi.
- **Heslo:** každý playlist má své a jde kdykoli změnit nebo odebrat. Po změně ho musí zadat
  všichni znovu. Prohlížeč si zadané heslo pamatuje 30 dní.
- **Přestat sdílet:** odkaz přestane platit a ze serveru se smaže audio, které nepoužívá jiný
  playlist.

## Co je potřeba vědět

- **Stažení nejde úplně zabránit.** Stránka nemá tlačítko pro stažení a audio není dostupné
  bez hesla, ale co prohlížeč přehraje, to si technicky zdatný posluchač umí uložit.
- **Autorská práva:** veřejně dostupná cizí hudba je riziko. Odkazy jsou proto náhodné a stránky
  mají `noindex`, ale odpovědnost za to, co a komu pošleš, je tvoje. Pro cizí hudbu používej heslo.
- **Nginx ignoruje `.htaccess`.** Zablokuj `lib` a `storage` v konfiguraci serveru, nebo přesuň
  audio mimo web pomocí `storage` v `config.php`. Soubory mají i tak náhodné názvy.

## Aktualizace a správa

- **Nová verze:** přepiš soubory, ale nech `config.php` a obsah `storage`.
- **Zapomenuté heslo k účtu:** vygeneruj nový hash příkazem
  `php -r 'echo password_hash("nove-heslo", PASSWORD_DEFAULT);'` a přepiš jím `password_hash`
  v tabulce `users`. Řádek nemaž, smazaly by se s ním i playlisty.
- **Odhlášení všech zařízení:** vymaž tabulku `api_tokens`.

## Test

`scripts/test-cloud.sh` spustí celý průchod proti dočasné MySQL v Dockeru.
