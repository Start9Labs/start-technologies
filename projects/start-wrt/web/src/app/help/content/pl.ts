// prettier-ignore
/** Polish help content (lazy-loaded on language switch). */
const HELP_PL: Record<string, string> = {
  '/devices': `## Urządzenia

Wyświetlaj i zarządzaj wszystkimi urządzeniami w sieci. Urządzenia online są obecnie połączone, urządzenia offline łączyły się wcześniej, ale nie są teraz aktywne.

### Nazwa

Nazwa hosta urządzenia lub przypisana przez Ciebie niestandardowa nazwa. Kliknij, aby wyświetlić szczegóły i skonfigurować urządzenie.

### Połączenie

Sposób, w jaki urządzenie łączy się z siecią (Ethernet, Wi-Fi).

### Adres MAC

Unikalny identyfikator sprzętowy interfejsu sieciowego urządzenia.

### Adres IP

Przypisane urządzeniu adresy IPv4 i IPv6.

### Dane i prędkość

Wykorzystanie przepustowości i bieżące prędkości transferu. Przydatne do identyfikacji intensywnych użytkowników sieci.`,
  '/devices/device': `## Urządzenia

Wyświetl informacje o urządzeniu i skonfiguruj ustawienia.

### Podsumowanie

Wyświetla bieżący stan urządzenia, typ połączenia, profil bezpieczeństwa oraz adresy IP. Ikona kłódki oznacza zarezerwowany adres IP.

### Wykorzystanie danych

Pokazuje historyczne wykorzystanie sieci przez to urządzenie. Użyj listy rozwijanej, aby wyświetlić różne okresy czasu. Pobieranie pokazane jest na niebiesko, wysyłanie na zielono.

### Nazwa

Przypisz niestandardową nazwę, aby łatwo zidentyfikować to urządzenie. Jeśli pozostawisz puste, użyta zostanie nazwa hosta urządzenia. Tekst zastępczy pokazuje, jaka nazwa zostanie zastosowana domyślnie.

### Zarezerwowany IP

Zarezerwuj adres IP, aby przypisać stały adres IPv4, który nie zmieni się między ponownymi uruchomieniami. Przydatne dla serwerów, drukarek, urządzeń NAS lub wszystkiego, do czego potrzebujesz dostępu pod spójnym adresem IP.

### Zapomnij

Usuwa urządzenie offline z listy urządzeń. Wszelkie niestandardowe nazwy lub ustawienia zarezerwowanego IP zostaną utracone. Jeśli urządzenie połączy się ponownie, pojawi się jako nowe urządzenie.`,
  '/ethernet': `## Ethernet

### Port

Fizyczny identyfikator portu Ethernet (np. eth0, eth1).

### Profil bezpieczeństwa

Domyślny profil bezpieczeństwa przypisany do urządzeń połączonych za pośrednictwem określonego portu Ethernet.

### WAN

Port sieci rozległej (WAN) łączy się z modemem ISP lub siecią nadrzędną. Tylko jeden port Ethernet może być w danym momencie wyznaczony jako WAN.

### Zmień port WAN

Użyj przycisku „Zmień port WAN”, aby przypisać oznaczenie WAN do innego portu Ethernet. Spowoduje to przerwanie połączenia internetowego i ponowne uruchomienie routera. Upewnij się, że modem jest podłączony do nowego portu WAN przed potwierdzeniem.`,
  '/ethernet/dialog': `## Zmień port WAN

Wybierz nowy port Ethernet, który będzie pełnił rolę połączenia WAN (internetowego). Bieżący port WAN jest wstępnie zaznaczony.

Zmiana portu WAN natychmiast przerwie połączenie internetowe i uruchomi ponownie router. Upewnij się, że modem lub kabel sieci nadrzędnej jest podłączony do nowego portu przed potwierdzeniem.`,
  '/inbound': `## Przychodzące VPN

Serwery VPN to aplikacje, które zapewniają bezpieczny i szyfrowany dostęp do wewnętrznych zasobów sieciowych, umożliwiając klientom tworzenie przychodzącego połączenia z siecią w bezpieczny sposób przez internet z dowolnego miejsca na świecie.

### Profil bezpieczeństwa

Wyświetla uprawnienia dostępu ustawione dla urządzenia. Zarządza i kontroluje, do czego urządzenie może mieć dostęp w sieci za pomocą profilu bezpieczeństwa lub harmonogramu.

### Port

Port sieciowy, na którym serwer VPN nasłuchuje połączeń przychodzących.`,
  '/inbound/dialog': `## Przychodzące VPN

### Etykieta

Jasna i opisowa nazwa serwera VPN, taka jak „Biurowy serwer VPN” lub „Domowy serwer VPN”, aby odróżnić go od innych połączeń VPN.

### Punkt końcowy

Adres IP lub nazwa domeny, pod którą serwer VPN jest osiągalny dla klientów łączących się przez internet.

### Profil bezpieczeństwa

Początkowy zestaw praw dostępu i ograniczeń przypisany klientom, gdy łączą się z serwerem VPN. Wybierz wcześniej utworzony profil lub harmonogram, albo utwórz nowy.

### Port

Port sieciowy, na którym serwer VPN nasłuchuje połączeń przychodzących. Powszechnie używanym portem dla WireGuard jest 51820.`,
  '/inbound/client': `## Przychodzące VPN

Zarządzaj urządzeniami klienckimi dla tego serwera VPN.

### Nazwa

Wybrana nazwa ułatwiająca identyfikację przeznaczenia urządzenia klienckiego.

### Adres IP LAN

Przypisany adres IP w sieci lokalnej. Został on skonfigurowany podczas konfigurowania urządzenia klienckiego.

### Routing

Pokazuje tryb routingu ruchu klienta. „Cały ruch” kieruje cały ruch internetowy przez tunel VPN. „Tylko LAN” kieruje przez tunel jedynie ruch sieci lokalnej. Użyj menu akcji, aby przełączać się między trybami.`,
  '/inbound/client/dialog-add': `## Przychodzące VPN

### Etykieta

Jasna i opisowa nazwa urządzenia klienckiego, taka jak „Laptop Jana” lub „Telefon biurowy”, aby odróżnić je od innych urządzeń.

### Adres IP LAN

Pożądany adres IP LAN urządzenia klienckiego, mieszczący się w zakresie IP przydzielonym dla VPN. Zapewnia, że urządzenie klienckie ma unikalny adres IP w sieci VPN, umożliwiając mu komunikację z innymi urządzeniami.

### Klucz publiczny

Klucz publiczny WireGuard dla urządzenia klienckiego. Jeśli pozostawisz puste, para kluczy zostanie wygenerowana automatycznie. Wprowadź istniejący klucz publiczny, jeśli urządzenie klienckie ma już skonfigurowany klucz.

### Kieruj cały ruch przez tunel

Po włączeniu cały ruch internetowy klienta jest kierowany przez tunel VPN. Po wyłączeniu (domyślnie) przez tunel przechodzi tylko ruch przeznaczony do sieci lokalnej, a klient korzysta z własnego połączenia internetowego dla wszystkiego pozostałego.`,
  '/inbound/client/dialog-config': `## Konfiguracja klienta

Konfiguracja WireGuard dla tego urządzenia klienckiego. Użyj jej, aby skonfigurować połączenie VPN na kliencie.

### Plik

Wyświetla konfigurację jako tekst. Użyj przycisku kopiowania, aby skopiować ją do schowka, lub przycisku pobierania, aby zapisać ją jako plik <code>wireguard.conf</code> , który można bezpośrednio zaimportować do aplikacji WireGuard.

### QR

Wyświetla konfigurację jako kod QR. Zeskanuj go aplikacją mobilną WireGuard, aby skonfigurować urządzenie klienckie bez ręcznego wprowadzania.`,
  '/inbound/client/dialog-rename': `## Zmień nazwę klienta

### Nazwa

Wprowadź nową nazwę dla tego urządzenia klienckiego VPN. Użyj czegoś opisowego, takiego jak „Laptop Jana” lub „Telefon biurowy”, aby łatwo zidentyfikować urządzenie na liście klientów.`,
  '/lan/ipv4': `## LAN – IPv4

### Blok sieci

Prywatny blok IP /16 dla Twojej sieci. Każdy profil bezpieczeństwa otrzyma własną podsieć /24 w obrębie tego bloku, co pozwala na maksymalnie 256 oddzielnych podsieci po 254 urządzenia każda.

### IP routera

Adres LAN routera to brama (.1) podsieci profilu bezpieczeństwa Admin. Pierwsze dwa oktety pochodzą z bloku sieci powyżej; trzeci oktet ustawia pole Podsieć profilu Admin, a nie ta strona.`,
  '/lan/ipv6': `## LAN – IPv6

### IPv6

Po włączeniu urządzenia w Twojej sieci będą otrzymywać adresy IPv6 za pośrednictwem SLAAC (bezstanowa autokonfiguracja adresów). Jest to standardowa metoda przypisywania adresów IPv6.

IPv6 umożliwia bezpośrednią osiągalność Twoich urządzeń z internetu bez NAT, co jest przydatne do hostowania usług lub aplikacji peer-to-peer.

### Długość prefiksu

Długość prefiksu określa rozmiar przestrzeni adresowej IPv6 Twojej sieci LAN. Musi być **większa** (wyższa liczba) niż prefiks WAN, aby utworzyć prawidłową podsieć.

Na przykład, jeśli Twój ISP przydziela Ci prefiks /48, możesz użyć /56, /60 lub /64 dla swojej sieci LAN. Dla większości sieci domowych zalecany jest prefiks /64.`,
  '/outbound': `## Wychodzące VPN

Wychodzące VPN kierują Twój ruch internetowy przez szyfrowane tunele do zdalnych serwerów, zwiększając prywatność i bezpieczeństwo.

### Po co używać wychodzących VPN?

-   Ukryj swój adres IP przed witrynami i usługami
-   Szyfruj ruch w niezaufanych sieciach
-   Uzyskaj dostęp do treści z ograniczeniami geograficznymi
-   Zapobiegaj śledzeniu przez ISP

### Łańcuchowanie VPN

Kieruj jeden VPN przez drugi w celu dodatkowej prywatności. Twój ruch wychodzi przez wiele serwerów, co utrudnia wyśledzenie go z powrotem do Ciebie. Pamiętaj, że łańcuchowanie zwiększa opóźnienia.

### Rozpoczęcie pracy

Prześlij plik konfiguracyjny WireGuard od swojego dostawcy VPN (Mullvad, ProtonVPN, IVPN itp.), aby utworzyć nowego klienta VPN.`,
  '/outbound/vpn': `## Wychodzące VPN

<details><summary>Etykieta</summary> Przyjazna nazwa identyfikująca to połączenie VPN. Użyj czegoś opisowego, jak „Mullvad Szwecja” lub „Praca VPN”. </details> <details><summary>Łączy się z</summary> Dokąd powinien być kierowany ruch z tego VPN:<ul><li><b>Internet:</b> Ruch wychodzi bezpośrednio do internetu przez ten VPN.</li><li><b>Inny VPN:</b> Najpierw połącz łańcuchowo przez inny VPN dla dodatkowej prywatności. Twój ruch zostanie zaszyfrowany wielokrotnie i wyjdzie przez wiele serwerów.</li></ul><b>Uwaga:</b> Łańcuchowanie VPN zwiększa opóźnienia i może zmniejszyć prędkości. </details> <details><summary>Ścieżka połączenia</summary> Pokazuje pełną trasę, jaką pokonuje Twój ruch od tego VPN do internetu. Na przykład: Mullvad → Proton → Internet oznacza, że ruch jest szyfrowany przez Mullvad, wysyłany przez Proton, a następnie wychodzi do internetu.</details>`,
  '/outbound/dialog': `## Wychodzące VPN

Klienci VPN tworzą wychodzące połączenie z serwerem VPN, umożliwiając urządzeniom wysyłanie i odbieranie danych tak, jakby były bezpośrednio połączone z siecią prywatną. Maskuje Twój adres IP, omija ograniczenia geograficzne i chroni poufne dane w publicznych sieciach Wi-Fi.

### Etykieta

Jasna i opisowa nazwa połączenia VPN, taka jak „Mullvad Szwecja” lub „Praca VPN”, aby łatwo odróżnić je od innych połączeń.

### Plik konfiguracyjny

Prześlij plik **.conf** WireGuard od swojego dostawcy VPN. Plik ten zawiera adres serwera, klucze i ustawienia połączenia potrzebne do nawiązania tunelu. Większość dostawców (Mullvad, ProtonVPN, IVPN itp.) udostępnia pliki konfiguracyjne WireGuard do pobrania ze swojej witryny lub aplikacji.

### Cel

Następny przeskok na ścieżce warstwowego połączenia VPN.`,
  '/profiles': `## Profile

Profile bezpieczeństwa definiują segmenty sieci z izolowanymi uprawnieniami. Każdy profil kontroluje dostęp do internetu i komunikację z innymi profilami w Twojej sieci lokalnej.

### Nazwa

Opisowa nazwa profilu bezpieczeństwa, taka jak „Goście” lub „Urządzenia IoT”.

### DNS

Których serwerów DNS używają urządzenia w tym profilu — systemowych („System”), własnych niestandardowych serwerów profilu („Niestandardowe”) lub, w przypadku kierowania przez VPN, serwerów VPN („VPN”).

### Wychodzący

Jak ruch z tego profilu dociera do internetu — bezpośrednio („Bezpośredni”) lub przez jednego z Twoich klientów VPN (pokazanego z nazwy).

### Dostęp LAN

Z którymi innymi profilami ten profil może się komunikować w sieci lokalnej — wszystkimi, tylko własnym lub wybraną białą listą.

### Dostęp WAN

Czy urządzenia w tym profilu mają dostęp do internetu, opcjonalnie ograniczony przez białą lub czarną listę adresów IP lub zakresów CIDR.`,
  '/profiles/dialog': `## Profile

Profile bezpieczeństwa definiują segmenty sieci z izolowanymi uprawnieniami. Każdy profil kontroluje dostęp do internetu i komunikację z innymi profilami w Twojej sieci lokalnej.

### Nazwa

Opisowa nazwa profilu bezpieczeństwa, taka jak „Goście” lub „Urządzenia IoT”.

### Podsieć

Trzeci oktet podsieci IPv4 dla tego profilu. Na przykład wartość 2 tworzy podsieć 192.168.2.0/24. Każdy profil musi mieć unikalną podsieć.

### Dostęp LAN

Kontroluje, z którymi innymi profilami ten profil może się komunikować w sieci lokalnej. Wybierz „Wszystkie” dla pełnego dostępu, „Ten sam profil” dla izolacji lub „Biała lista”, aby wybrać poszczególne profile.

### Automatycznie dodawaj nowe profile do białej listy

Gdy dostęp LAN jest ustawiony na „Biała lista”, włączenie tej opcji przyznaje również dostęp do wszelkich profili utworzonych w przyszłości — przydatne dla profili administracyjnych, które powinny mieć dostęp do wszystkich segmentów sieci.

### Routing wychodzący

Wybierz, jak ruch z tego profilu dociera do internetu. Wybierz „Bezpośredni” dla normalnego dostępu do internetu lub „VPN”, aby kierować cały ruch profilu przez jednego z Twoich klientów VPN.

### Dostęp WAN

Czy urządzenia w tym profilu mają dostęp do internetu. Użyj „Białej listy” lub „Czarnej listy”, aby ograniczyć dostęp do określonych adresów IP lub zakresów CIDR, albo „Brak”, aby całkowicie zablokować internet.

### Harmonogram wyłączeń

Wyznaczone czasy, kiedy dostęp WAN (internetowy) jest zablokowany dla urządzeń w tym profilu. Poza tymi oknami obowiązują normalne reguły dostępu WAN profilu. Na przykład profil „Dzieci” może blokować dostęp WAN o 19:00 w dni szkolne i o 21:00 w weekendy, podczas gdy profil „Admin” zachowuje pełny dostęp 24/7.

Ta sekcja jest wyłączona, gdy dostęp WAN jest ustawiony na „Brak” — internet jest już zablokowany przez cały czas, więc nie ma czego harmonogramować. Wszelkie okna, które skonfigurowałeś, są zachowywane i ponownie zaczynają obowiązywać po włączeniu dostępu WAN.

### DNS

Wybierz, czy ten profil używa systemowych serwerów DNS, czy własnych. Wybierz „Niestandardowe”, aby określić do trzech serwerów DNS, każdy z opcjonalnym szyfrowaniem DoH (DNS over HTTPS) dla bezpiecznych zapytań.`,
  '/profiles/blackout': `## Profile – Czasy wyłączenia

Okno wyłączenia blokuje dostęp WAN (internetowy) dla wszystkich urządzeń w tym profilu w określonym okresie czasu. Poza oknami wyłączenia obowiązują normalne reguły dostępu WAN profilu.

### Okno czasowe

Czas rozpoczęcia i zakończenia okresu wyłączenia. Dostęp WAN będzie zablokowany między tymi godzinami w wybrane dni. Czas zakończenia musi być późniejszy niż czas rozpoczęcia.

### Dni

Wybierz, w które dni tygodnia okno wyłączenia ma być aktywne.`,
  '/profiles/schedule': `### Harmonogram wyłączeń

Wyznaczone czasy, kiedy dostęp WAN (internetowy) jest zablokowany dla urządzeń w tym profilu. Poza tymi oknami obowiązują normalne reguły dostępu WAN profilu. Na przykład profil „Dzieci” może blokować dostęp WAN o 19:00 w dni szkolne i o 21:00 w weekendy, podczas gdy profil „Admin” zachowuje pełny dostęp 24/7.

Kliknij +, aby dodać okno, kliknij dwukrotnie istniejące okno, aby je edytować lub usunąć, albo przeciągnij jego uchwyty, aby zmienić jego rozmiar w miejscu.`,
  '/published-ports': `## Opublikowane porty

### Opublikowane porty

Udostępnij wybrane porty urządzenia z internetu. Ruch z internetu na określonym porcie zostanie przekierowany do wybranego urządzenia.

### Etykieta

Opisowa nazwa pomagająca zidentyfikować tę regułę, taka jak „Home Assistant” lub „Serwer Minecraft”.

### Urządzenie

Wybranemu urządzeniu zostanie przypisany stały adres IPv4, aby zapewnić, że reguła przekierowania portu zawsze dociera do zamierzonego urządzenia. Adresy IPv6 są zazwyczaj stabilne dzięki SLAAC.

### Port

Port lub zakres portów urządzenia do udostępnienia. Na przykład „443” dla pojedynczego portu lub „27015-27030” dla zakresu.

### Protokół

Wybierz TCP dla większości usług (web, SSH itp.), UDP dla gier lub VoIP albo oba, jeśli usługa tego wymaga.

### Źródło

Ogranicz, kto może się łączyć, określając adres IP lub zakres CIDR. Użyj „Dowolne”, aby zezwolić na połączenia z dowolnego miejsca w internecie.

### Punkty końcowe

Publiczne adresy, pod którymi ten port jest osiągalny. Punkty końcowe IPv4 używają NAT do przekierowywania ruchu, podczas gdy IPv6 otwiera zaporę bezpośrednio do urządzenia.`,
  '/published-ports/dialog': `## Opublikowane porty

Udostępnij wybrane porty urządzenia z internetu. Ruch z internetu na określonym porcie zostanie przekierowany do wybranego urządzenia.

### Etykieta

Opisowa nazwa pomagająca zidentyfikować tę regułę, taka jak „Home Assistant” lub „Serwer Minecraft”.

### Urządzenie

Urządzenie, do którego ma być przekierowywany ruch. Wybranemu urządzeniu zostanie przypisany stały adres IPv4, aby zapewnić, że reguła zawsze dociera do zamierzonego urządzenia.

### Port

Port lub zakres portów urządzenia do udostępnienia. Na przykład „443” dla pojedynczego portu lub „27015-27030” dla zakresu.

### Protokół

Wybierz TCP dla większości usług (web, SSH itp.), UDP dla gier lub VoIP albo oba, jeśli usługa tego wymaga.

### Źródło

Ogranicz, kto może się łączyć, określając adres IP lub zakres CIDR. Użyj „Dowolne”, aby zezwolić na połączenia z dowolnego miejsca w internecie.

### Wersja IP

Wybierz, na których wersjach IP publikować. IPv4 używa NAT do przekierowywania ruchu, podczas gdy IPv6 otwiera zaporę bezpośrednio do urządzenia.

### Port zewnętrzny

Numer portu widoczny dla świata zewnętrznego. Użyj „Taki sam jak urządzenia”, aby pozostawić go identycznym, lub określ inny port dla dostępu zewnętrznego.`,
  '/wan/ipv4': `## WAN – IPv4

### Adres IP

-   **DHCP:** Twój ISP automatycznie przypisuje adres IP Twojemu routerowi. Jest to najczęstsza konfiguracja.
-   **PPPoE:** Używany przez niektórych dostawców DSL. Wymaga nazwy użytkownika i hasła od Twojego ISP.
-   **Static:** Ręcznie skonfiguruj stały adres IP. Używaj tylko, jeśli Twój ISP przypisał Ci statyczny adres IP.`,
  '/wan/ipv6': `## WAN – IPv6

### Adres IP

-   **SLAAC:** Automatyczna konfiguracja IPv6. Najczęstsza opcja, jeśli Twój ISP obsługuje IPv6.
-   **DHCPv6:** ISP przypisuje adres IPv6 za pośrednictwem DHCP. Użyj, jeśli SLAAC nie działa z Twoim ISP.
-   **Static:** Ręcznie skonfiguruj stały adres IPv6 przypisany przez Twojego ISP.
-   **6RD:** Tuneluje IPv6 przez połączenie IPv4. Wymaga szczegółów konfiguracji od Twojego ISP.
-   **Wyłączone:** Wyłącza IPv6 na interfejsie WAN.

### Prefiks IPv6

Dla SLAAC i DHCPv6 prefiks określa żądaną długość prefiksu dla delegacji prefiksu (np. /48, /56, /64). Pozostaw puste, aby Twój ISP zdecydował automatycznie.

### DNS

DNS (system nazw domen) tłumaczy nazwy domen na adresy IP.

-   **Pobierz od ISP:** Użyj serwerów DNS dostarczanych automatycznie przez Twojego ISP.
-   **Niestandardowe:** Określ własne serwery DNS. Obsługiwane są zarówno adresy IPv4, jak i IPv6.`,
  '/wan/mac-address': `## WAN – Adres MAC

### Adres MAC

Adres MAC identyfikuje Twój router wobec Twojego ISP. Niektórzy ISP blokują usługę do określonego adresu MAC.

-   **Router:** Użyj wbudowanego adresu MAC routera.
-   **Niestandardowy:** Sklonuj adres MAC poprzedniego urządzenia. Przydatne, jeśli Twój ISP zablokował usługę do adresu MAC Twojego starego routera lub modemu.`,
  '/wan/dns': `## WAN – DNS

### DNS

DNS (system nazw domen) tłumaczy nazwy domen na adresy IP.

-   **Pobierz od ISP:** Użyj serwerów DNS dostarczanych automatycznie przez Twojego ISP.
-   **Niestandardowe:** Określ własne serwery DNS (np. 1.1.1.1, 8.8.8.8).

### Bezpieczny (DoH)

Włącz DNS over HTTPS (DoH) dla szyfrowanych zapytań DNS. Poprawia to prywatność, zapobiegając podsłuchiwaniu Twojego ruchu DNS.

**Uwaga:** Nie wszystkie serwery DNS obsługują DoH. Powszechne serwery, które to robią, to Cloudflare (1.1.1.1), Google (8.8.8.8) i Quad9 (9.9.9.9).`,
  '/wan/dynamic-dns': `## WAN – Dynamiczny DNS

### Dynamiczny DNS

DDNS mapuje Twój zmieniający się adres IP na stałą nazwę domeny, umożliwiając zdalny dostęp do Twojej sieci bez znajomości bieżącego adresu IP.

-   **Start9:** Darmowa usługa DDNS dostarczana przez Start9. Nie wymaga dodatkowej konfiguracji.
-   **Inni dostawcy:** Wymaga konta u dostawcy. Wprowadź dane uwierzytelniające i nazwę hosta od swojego dostawcy DDNS.`,
  '/settings/activity': `## Ustawienia – Aktywność

Wyświetl ostatnie próby logowania i zdarzenia bezpieczeństwa. Monitoruj podejrzaną aktywność i usuwaj stare wpisy w razie potrzeby.`,
  '/settings/advanced': `## Ustawienia – Logi

Zaawansowane opcje dla zaawansowanych użytkowników. LuCI zapewnia bezpośredni dostęp do konfiguracji OpenWRT. Diagnostyka wsparcia pomaga rozwiązywać problemy. Przywracanie ustawień fabrycznych przywraca ustawienia domyślne.`,
  '/settings/backup': `## Ustawienia – Kopia zapasowa

### Utwórz kopię zapasową

Pobierz kopię zapasową wszystkich ustawień routera, w tym profili bezpieczeństwa, konfiguracji WiFi, certyfikatów SSL i preferencji systemowych. Przechowuj plik kopii zapasowej w bezpiecznym miejscu.

### Przywróć kopię zapasową

Prześlij wcześniej utworzony plik kopii zapasowej, aby przywrócić wszystkie ustawienia. Router uruchomi się ponownie po przywróceniu, a wszystkie bieżące ustawienia zostaną nadpisane.`,
  '/settings/general': `## Ustawienia – Ogólne

<details><summary>Preferencje</summary> Ogólne preferencje dotyczące interfejsu użytkownika i zachowania routera.<h3>Motyw</h3>Wybierz motyw ciemny lub jasny albo zachowaj ustawienia systemowe zastosowane do interfejsu.<h3>Język</h3>Skonfiguruj preferowany język interfejsu użytkownika routera. To ustawienie wpływa na język wyświetlany dla wszystkich menu, opcji i innych elementów tekstowych w interfejsie internetowym routera.<h3>Strefa czasowa</h3>Ustaw lokalną strefę czasową dla routera. Wpływa to na zaplanowane zdarzenia, takie jak okna wyłączenia WAN i harmonogramy wyłączeń WiFi. </details> <details><summary>Dostęp zdalny</summary> Umożliwia zdalny dostęp do zarządzania ustawieniami i konfiguracjami routera spoza sieci lokalnej.<ul><li><b>Gdy za NAT</b> (translacja adresów sieciowych): Zezwala na zdalny dostęp tylko wtedy, gdy router jest częścią sieci prywatnej korzystającej z NAT. NAT jest powszechnie używany w sieciach domowych i małych firm, gdzie wiele urządzeń współdzieli jeden publiczny adres IP.</li><li><b>Nigdy:</b> Całkowicie wyłącza zdalny dostęp. Odpowiednie dla środowisk, w których bezpieczeństwo jest najważniejsze, takich jak wysoce wrażliwe lub izolowane sieci.</li><li><b>Zawsze:</b> Włącza zdalny dostęp przez cały czas. Idealne dla środowisk, w których konieczne jest ciągłe zdalne zarządzanie, takich jak sieć firmowa, gdzie administratorzy IT muszą utrzymywać stały dostęp do routera w celu monitorowania i aktualizacji.</li></ul></details><details><summary>Bezpieczeństwo</summary> Pobierz certyfikat Root CA, aby zaufać połączeniom HTTPS z dodatkowych urządzeń. Zainstalowanie tego certyfikatu pozwala przeglądarkom i aplikacjom zweryfikować tożsamość routera bez ostrzeżeń bezpieczeństwa. </details> <details><summary>Aktualizacje</summary> Gdy dostępna jest aktualizacja, na górze tej strony pojawi się baner. Możesz wyświetlić informacje o wydaniu przed aktualizacją. Router będzie miał krótki przestój podczas procesu aktualizacji.</details>`,
  '/settings/logs': `## Ustawienia – Logi

Logi rejestrują zdarzenia systemowe i pomagają diagnozować problemy. Przydatne do rozwiązywania problemów i zapytań do wsparcia.`,
  '/settings/password': `## Ustawienia – Hasło

Zmienia hasło administratora routera. Zwiększa bezpieczeństwo poprzez ochronę ustawień routera, zapobiegając nieautoryzowanemu dostępowi. Ustaw silne, unikalne hasło.`,
  '/settings/ssh-keys': `## Ustawienia – Klucze SSH

Klucze SSH umożliwiają bezpieczne uwierzytelnianie bez hasła do Twojego routera. Dodaj klucze publiczne użytkowników, którzy potrzebują dostępu SSH.`,
  '/settings/ssh-keys/dialog': `## Ustawienia – Klucze SSH

Klucze SSH umożliwiają bezpieczne uwierzytelnianie bez hasła do Twojego routera. Dodaj klucz publiczny użytkownika, który potrzebuje dostępu SSH.

### Klucz publiczny

Wklej pełny ciąg klucza publicznego, w tym prefiks algorytmu i dane klucza. Obsługiwane formaty to ssh-ed25519, ssh-rsa oraz ecdsa-sha2-nistp256/384/521.`,
  '/wifi/blackout-schedule': `## Wi-Fi – Harmonogram wyłączeń

Określa wyznaczone czasy, kiedy WiFi będzie wyłączone, uniemożliwiając jakimkolwiek urządzeniom łączenie się z nim. Funkcja ta jest często używana do ograniczania dostępu do internetu w określonych okresach w różnych celach, takich jak redukowanie rozproszeń, egzekwowanie rutyny pory snu lub zwiększanie bezpieczeństwa sieci w godzinach poza pracą.

**Kliknij dwukrotnie istniejące interwały, aby je edytować lub usunąć**

Przeciągnij istniejące interwały, aby edytować je w miejscu.`,
  '/wifi/blackout-schedule/dialog': `## Wi-Fi – Harmonogram wyłączeń

Okno wyłączenia dezaktywuje WiFi w określonym okresie czasu, uniemożliwiając jakimkolwiek urządzeniom łączenie się. Przydatne do redukowania rozproszeń, egzekwowania rutyny pory snu lub zwiększania bezpieczeństwa sieci w godzinach poza pracą.

### Okno czasowe

Czas rozpoczęcia i zakończenia okresu wyłączenia. WiFi będzie wyłączone między tymi godzinami w wybrane dni. Czas zakończenia musi być późniejszy niż czas rozpoczęcia.

### Dni

Wybierz, w które dni tygodnia okno wyłączenia ma być aktywne.`,
  '/wifi/passwords': `## Wi-Fi – Hasła

Zarządzaj hasłami sieci bezprzewodowych. Każdy wpis reprezentuje sieć Wi-Fi z własnym hasłem i profilem bezpieczeństwa kontrolującym uprawnienia dostępu.

### Profil bezpieczeństwa

Kontroluje, do czego podłączone urządzenie może mieć dostęp w sieci. Przypisz profil bezpieczeństwa, aby ograniczyć lub przyznać dostęp do określonych zasobów.`,
  '/wifi/passwords/dialog': `## Wi-Fi – Hasła

Każdy wpis reprezentuje sieć Wi-Fi z własnym hasłem i profilem bezpieczeństwa kontrolującym uprawnienia dostępu.

### Etykieta

Opisowa nazwa tej sieci Wi-Fi, taka jak „Dom” lub „Sieć dla gości”. Jest to nazwa sieci (SSID), którą zobaczą urządzenia.

### Hasło

Hasło, którego urządzenia będą używać do łączenia się. Musi mieć co najmniej 8 znaków. Użyj przycisku generowania, aby utworzyć silne losowe hasło.

### Profil bezpieczeństwa

Kontroluje, do czego podłączone urządzenie może mieć dostęp w sieci. Przypisz profil bezpieczeństwa, aby ograniczyć lub przyznać dostęp do określonych zasobów.`,
  '/wifi/settings': `## Wi-Fi – Ustawienia

### Włącz Wi-Fi

Włącz lub wyłącz radio bezprzewodowe. Po wyłączeniu żadne urządzenie nie może połączyć się przez Wi-Fi.

### SSID

SSID (identyfikator zestawu usług) to nazwa Twojej sieci WiFi. Identyfikuje Twoją sieć wobec urządzeń, umożliwiając im znalezienie i połączenie się z siecią.

### Rozgłaszanie

Rozgłaszanie SSID sprawia, że sieć jest wykrywalna przez urządzenia poszukujące połączeń bezprzewodowych. Wyłączenie rozgłaszania ukrywa sieć przed przypadkowymi użytkownikami, dodając warstwę bezpieczeństwa.

### Pasmo częstotliwości

Pasmo częstotliwości, na którym działa Twoje WiFi. 2,4 GHz oferuje lepszy zasięg, podczas gdy 5 GHz oferuje wyższe prędkości. Wybierz „Oba”, aby włączyć działanie dwupasmowe.

### Rozgłaszaj osobno

Podczas korzystania z obu pasm częstotliwości ta opcja tworzy osobne SSID dla 2,4 GHz i 5 GHz (np. „MojaSieć” i „MojaSieć-5G”). Przydatne, jeśli chcesz jawnie kontrolować, z którym pasmem łączy się urządzenie.

### Kanały

Konkretne kanały w obrębie każdego pasma częstotliwości. Wybierz „Auto”, aby router wybrał najlepszy kanał, lub wybierz konkretny kanał, aby uniknąć zakłóceń od sąsiednich sieci.`,
}

export default HELP_PL
