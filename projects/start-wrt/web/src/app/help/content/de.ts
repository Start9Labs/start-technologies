// prettier-ignore
/** German help content (lazy-loaded on language switch). */
const HELP_DE: Record<string, string> = {
  '/devices': `## Geräte

Alle Geräte in Ihrem Netzwerk anzeigen und verwalten. Online-Geräte sind derzeit verbunden, Offline-Geräte waren zuvor verbunden, sind aber aktuell nicht aktiv.

### Name

Der Hostname des Geräts oder ein benutzerdefinierter Name, den Sie vergeben haben. Klicken Sie, um Details anzuzeigen und das Gerät zu konfigurieren.

### Verbindung

Wie das Gerät mit Ihrem Netzwerk verbunden ist (Ethernet, Wi-Fi).

### MAC-Adresse

Die eindeutige Hardware-Kennung der Netzwerkschnittstelle des Geräts.

### IP-Adresse

Die dem Gerät zugewiesenen IPv4- und IPv6-Adressen.

### Daten & Geschwindigkeit

Bandbreitennutzung und aktuelle Übertragungsgeschwindigkeiten. Nützlich, um Geräte mit hohem Netzwerkverbrauch zu identifizieren.`,
  '/devices/device': `## Geräte

Geräteinformationen anzeigen und Einstellungen konfigurieren.

### Übersicht

Zeigt den aktuellen Status des Geräts, den Verbindungstyp, das Sicherheitsprofil und die IP-Adressen an. Ein Schloss-Symbol kennzeichnet eine reservierte IP-Adresse.

### Datennutzung

Zeigt die bisherige Netzwerknutzung dieses Geräts an. Verwenden Sie das Dropdown-Menü, um verschiedene Zeiträume anzuzeigen. Der Download wird in Blau, der Upload in Grün dargestellt.

### Name

Vergeben Sie einen benutzerdefinierten Namen, um dieses Gerät einfach zu identifizieren. Bleibt das Feld leer, wird der Hostname des Geräts verwendet. Der Platzhalter zeigt an, welcher Name standardmäßig verwendet wird.

### Reservierte IP

Reservieren Sie eine IP-Adresse, um eine feste IPv4-Adresse zuzuweisen, die sich zwischen Neustarts nicht ändert. Nützlich für Server, Drucker, NAS-Geräte oder alles, was Sie über eine gleichbleibende IP-Adresse erreichen müssen.

### Entfernen

Entfernt ein Offline-Gerät aus Ihrer Geräteliste. Alle benutzerdefinierten Namen oder reservierten IP-Einstellungen gehen dabei verloren. Verbindet sich das Gerät erneut, erscheint es als neues Gerät.`,
  '/ethernet': `## Ethernet

### Port

Die Kennung des physischen Ethernet-Ports (z. B. eth0, eth1).

### Sicherheitsprofil

Das standardmäßige Sicherheitsprofil, das Geräten zugewiesen wird, die über den angegebenen Ethernet-Port verbunden sind.

### WAN

Der WAN-Port (Wide Area Network) verbindet sich mit dem Modem Ihres ISP oder dem vorgelagerten Netzwerk. Es kann immer nur ein Ethernet-Port als WAN festgelegt werden.

### WAN-Port ändern

Verwenden Sie die Schaltfläche „WAN-Port ändern“, um die WAN-Zuordnung einem anderen Ethernet-Port zuzuweisen. Dies unterbricht Ihre Internetverbindung und startet den Router neu. Stellen Sie sicher, dass Ihr Modem mit dem neuen WAN-Port verbunden ist, bevor Sie bestätigen.`,
  '/ethernet/dialog': `## WAN-Port ändern

Wählen Sie einen neuen Ethernet-Port, der als WAN-Verbindung (Internet) dienen soll. Der aktuelle WAN-Port ist vorausgewählt.

Das Ändern des WAN-Ports unterbricht sofort Ihre Internetverbindung und startet den Router neu. Stellen Sie sicher, dass Ihr Modem oder das vorgelagerte Netzwerkkabel mit dem neuen Port verbunden ist, bevor Sie bestätigen.`,
  '/inbound': `## Eingehende VPNs

VPN-Server sind Software-Anwendungen, die sicheren und verschlüsselten Zugriff auf interne Netzwerkressourcen bereitstellen und es Clients ermöglichen, eine eingehende Verbindung zum Netzwerk sicher über das Internet von überall auf der Welt herzustellen.

### Sicherheitsprofil

Zeigt die für das Gerät festgelegten Zugriffsberechtigungen an. Verwaltet und steuert, worauf das Gerät im Netzwerk über ein Sicherheitsprofil oder einen Zeitplan zugreifen kann.

### Port

Der Netzwerk-Port, auf dem der VPN-Server auf eingehende Verbindungen wartet.`,
  '/inbound/dialog': `## Eingehende VPNs

### Bezeichnung

Ein klarer und aussagekräftiger Name für den VPN-Server, etwa „Büro-VPN-Server“ oder „Heim-VPN-Server“, um ihn von anderen VPN-Verbindungen zu unterscheiden.

### Endpunkt

Die IP-Adresse oder der Domänenname, unter dem der VPN-Server von Clients erreichbar ist, die sich über das Internet verbinden.

### Sicherheitsprofil

Der anfängliche Satz an Zugriffsrechten und Einschränkungen, der Clients zugewiesen wird, wenn sie sich mit dem VPN-Server verbinden. Wählen Sie ein zuvor erstelltes Profil oder einen Zeitplan, oder erstellen Sie ein neues.

### Port

Der Netzwerk-Port, auf dem der VPN-Server auf eingehende Verbindungen wartet. Der gebräuchliche Port für WireGuard ist 51820.`,
  '/inbound/client': `## Eingehende VPNs

Client-Geräte für diesen VPN-Server verwalten.

### Name

Ein gewählter Name, um den Zweck des Client-Geräts einfach zu identifizieren.

### LAN-IP-Adresse

Die zugewiesene IP-Adresse im lokalen Netzwerk. Diese wurde beim Einrichten des Client-Geräts konfiguriert.

### Routing

Zeigt den Routing-Modus des Client-Datenverkehrs an. „Gesamter Datenverkehr“ leitet den gesamten Internetverkehr durch den VPN-Tunnel. „Nur LAN“ leitet nur den lokalen Netzwerkverkehr durch den Tunnel. Verwenden Sie das Aktionsmenü, um zwischen den Modi zu wechseln.`,
  '/inbound/client/dialog-add': `## Eingehende VPNs

### Bezeichnung

Ein klarer und aussagekräftiger Name für das Client-Gerät, etwa „Johns Laptop“ oder „Büro-Telefon“, um es von anderen Geräten zu unterscheiden.

### LAN-IP-Adresse

Die gewünschte LAN-IP-Adresse für das Client-Gerät; stellen Sie sicher, dass sie innerhalb des für das VPN zugewiesenen IP-Bereichs liegt. Stellt sicher, dass das Client-Gerät eine eindeutige IP-Adresse innerhalb des VPN-Netzwerks erhält, sodass es mit anderen Geräten kommunizieren kann.

### Öffentlicher Schlüssel

Der öffentliche WireGuard-Schlüssel für das Client-Gerät. Bleibt das Feld leer, wird automatisch ein Schlüsselpaar generiert. Geben Sie einen vorhandenen öffentlichen Schlüssel ein, wenn das Client-Gerät bereits einen konfiguriert hat.

### Gesamten Datenverkehr durch den Tunnel leiten

Wenn aktiviert, wird der gesamte Internetverkehr des Clients durch den VPN-Tunnel geleitet. Wenn deaktiviert (Standard), läuft nur der für das lokale Netzwerk bestimmte Datenverkehr durch den Tunnel, und der Client nutzt für alles andere seine eigene Internetverbindung.`,
  '/inbound/client/dialog-config': `## Client-Konfiguration

Die WireGuard-Konfiguration für dieses Client-Gerät. Verwenden Sie sie, um die VPN-Verbindung auf dem Client einzurichten.

### Datei

Zeigt die Konfiguration als Text an. Verwenden Sie die Schaltfläche zum Kopieren, um sie in Ihre Zwischenablage zu kopieren, oder die Schaltfläche zum Herunterladen, um sie als <code>wireguard.conf</code> -Datei zu speichern, die sich direkt in die WireGuard-App importieren lässt.

### QR

Zeigt die Konfiguration als QR-Code an. Scannen Sie ihn mit der mobilen WireGuard-App, um das Client-Gerät ohne manuelle Eingabe zu konfigurieren.`,
  '/inbound/client/dialog-rename': `## Client umbenennen

### Name

Geben Sie einen neuen Namen für dieses VPN-Client-Gerät ein. Verwenden Sie etwas Aussagekräftiges, etwa „Johns Laptop“ oder „Büro-Telefon“, um das Gerät in Ihrer Client-Liste einfach zu identifizieren.`,
  '/lan/ipv4': `## LAN – IPv4

### Netzwerkblock

Der private /16-IP-Block für Ihr Netzwerk. Jedes Sicherheitsprofil erhält sein eigenes /24-Subnetz innerhalb dieses Blocks, was bis zu 256 separate Subnetze mit jeweils 254 Geräten ermöglicht.

### Router-IP

Die LAN-IP des Routers ist das Gateway (.1) des Subnetzes des Admin-Sicherheitsprofils. Die ersten beiden Oktette stammen aus dem Netzwerkblock oben; das dritte Oktett wird über das Subnetz-Feld des Admin-Profils festgelegt, nicht auf dieser Seite.`,
  '/lan/ipv6': `## LAN – IPv6

### IPv6

Wenn aktiviert, erhalten Geräte in Ihrem Netzwerk IPv6-Adressen über SLAAC (Stateless Address Autoconfiguration). Dies ist die Standardmethode für die IPv6-Adresszuweisung.

IPv6 ermöglicht es, dass Ihre Geräte ohne NAT direkt aus dem Internet erreichbar sind, was nützlich ist, um Dienste zu hosten oder für Peer-to-Peer-Anwendungen.

### Präfixlänge

Die Präfixlänge bestimmt die Größe des IPv6-Adressraums Ihres LAN. Sie muss **größer** (eine höhere Zahl) als Ihr WAN-Präfix sein, um ein gültiges Subnetz zu bilden.

Weist Ihnen Ihr ISP beispielsweise ein /48-Präfix zu, können Sie /56, /60 oder /64 für Ihr LAN verwenden. Für die meisten Heimnetzwerke wird ein /64-Präfix empfohlen.`,
  '/outbound': `## Ausgehende VPNs

Ausgehende VPNs leiten Ihren Internetverkehr durch verschlüsselte Tunnel zu entfernten Servern und verbessern so Privatsphäre und Sicherheit.

### Warum ausgehende VPNs verwenden?

-   Ihre IP-Adresse vor Websites und Diensten verbergen
-   Datenverkehr in nicht vertrauenswürdigen Netzwerken verschlüsseln
-   Auf geografisch eingeschränkte Inhalte zugreifen
-   Verfolgung durch den ISP verhindern

### VPN-Verkettung

Leiten Sie ein VPN durch ein anderes für zusätzliche Privatsphäre. Ihr Datenverkehr verlässt das Netz über mehrere Server, wodurch es schwieriger wird, ihn bis zu Ihnen zurückzuverfolgen. Beachten Sie, dass die Verkettung die Latenz erhöht.

### Erste Schritte

Laden Sie eine WireGuard-Konfigurationsdatei von Ihrem VPN-Anbieter (Mullvad, ProtonVPN, IVPN usw.) hoch, um einen neuen VPN-Client zu erstellen.`,
  '/outbound/vpn': `## Ausgehende VPNs

<details><summary>Bezeichnung</summary> Ein einprägsamer Name zur Identifizierung dieser VPN-Verbindung. Verwenden Sie etwas Aussagekräftiges wie „Mullvad Schweden“ oder „Arbeits-VPN“. </details> <details><summary>Verbindet sich mit</summary> Wohin der Datenverkehr von diesem VPN geleitet werden soll:<ul><li><b>Internet:</b> Der Datenverkehr gelangt über dieses VPN direkt ins Internet.</li><li><b>Ein weiteres VPN:</b> Zuerst durch ein weiteres VPN verketten für zusätzliche Privatsphäre. Ihr Datenverkehr wird mehrfach verschlüsselt und verlässt das Netz über mehrere Server.</li></ul><b>Hinweis:</b> Das Verketten von VPNs erhöht die Latenz und kann die Geschwindigkeit verringern. </details> <details><summary>Verbindungspfad</summary> Zeigt den vollständigen Weg, den Ihr Datenverkehr von diesem VPN ins Internet nimmt. Zum Beispiel: Mullvad → Proton → Internet bedeutet, dass der Datenverkehr von Mullvad verschlüsselt, durch Proton gesendet wird und dann ins Internet gelangt.</details>`,
  '/outbound/dialog': `## Ausgehende VPNs

VPN-Clients stellen eine ausgehende Verbindung zu einem VPN-Server her und ermöglichen es Geräten, Daten so zu senden und zu empfangen, als wären sie direkt mit dem privaten Netzwerk verbunden. Verbirgt Ihre IP-Adresse, umgeht geografische Beschränkungen und schützt sensible Daten in öffentlichen Wi-Fi-Netzwerken.

### Bezeichnung

Ein klarer und aussagekräftiger Name für die VPN-Verbindung, etwa „Mullvad Schweden“ oder „Arbeits-VPN“, um sie einfach von anderen Verbindungen zu unterscheiden.

### Konfigurationsdatei

Laden Sie eine WireGuard- **.conf** -Datei von Ihrem VPN-Anbieter hoch. Diese Datei enthält die Serveradresse, die Schlüssel und die Verbindungseinstellungen, die zum Aufbau des Tunnels erforderlich sind. Die meisten Anbieter (Mullvad, ProtonVPN, IVPN usw.) bieten WireGuard-Konfigurationsdateien zum Download auf ihrer Website oder in ihrer App an.

### Ziel

Der nächste Knoten im Pfad einer mehrschichtigen VPN-Verbindung.`,
  '/profiles': `## Profile

Sicherheitsprofile definieren Netzwerksegmente mit isolierten Berechtigungen. Jedes Profil steuert den Internetzugriff und die Kommunikation mit anderen Profilen in Ihrem lokalen Netzwerk.

### Name

Ein aussagekräftiger Name für das Sicherheitsprofil, etwa „Gast“ oder „IoT-Geräte“.

### DNS

Welche DNS-Server Geräte in diesem Profil verwenden – die des Systems („System“), die eigenen benutzerdefinierten Server des Profils („Benutzerdefiniert“) oder, beim Routing durch ein VPN, die des VPN („VPN“).

### Ausgehend

Wie der Datenverkehr von diesem Profil ins Internet gelangt – direkt („Direkt“) oder durch einen Ihrer VPN-Clients (nach Name angezeigt).

### LAN-Zugriff

Mit welchen anderen Profilen dieses Profil im lokalen Netzwerk kommunizieren kann – mit allen, nur mit dem eigenen oder einer gewählten Whitelist.

### WAN-Zugriff

Ob Geräte in diesem Profil Internetzugriff haben, optional eingeschränkt durch eine Whitelist oder Blacklist von IP-Adressen oder CIDR-Bereichen.`,
  '/profiles/dialog': `## Profile

Sicherheitsprofile definieren Netzwerksegmente mit isolierten Berechtigungen. Jedes Profil steuert den Internetzugriff und die Kommunikation mit anderen Profilen in Ihrem lokalen Netzwerk.

### Name

Ein aussagekräftiger Name für das Sicherheitsprofil, etwa „Gast“ oder „IoT-Geräte“.

### Subnetz

Das dritte Oktett des IPv4-Subnetzes für dieses Profil. Ein Wert von 2 erzeugt beispielsweise das Subnetz 192.168.2.0/24. Jedes Profil muss ein eindeutiges Subnetz haben.

### LAN-Zugriff

Steuert, mit welchen anderen Profilen dieses Profil im lokalen Netzwerk kommunizieren kann. Wählen Sie „Alle“ für vollen Zugriff, „Gleiches Profil“ für Isolierung oder „Whitelist“, um einzelne Profile auszuwählen.

### Neue Profile automatisch zur Whitelist hinzufügen

Wenn der LAN-Zugriff auf „Whitelist“ gesetzt ist, gewährt das Aktivieren dieser Option auch Zugriff auf alle künftig erstellten Profile – nützlich für Admin-Profile, die alle Netzwerksegmente erreichen sollen.

### Ausgehendes Routing

Wählen Sie, wie der Datenverkehr von diesem Profil ins Internet gelangt. Wählen Sie „Direkt“ für normalen Internetzugriff oder „VPN“, um den gesamten Datenverkehr des Profils durch einen Ihrer VPN-Clients zu leiten.

### WAN-Zugriff

Ob Geräte in diesem Profil Internetzugriff haben. Verwenden Sie „Whitelist“ oder „Blacklist“, um den Zugriff auf bestimmte IP-Adressen oder CIDR-Bereiche zu beschränken, oder „Keiner“, um das Internet vollständig zu blockieren.

### Sperrzeitplan

Festgelegte Zeiten, zu denen der WAN-Zugriff (Internet) für Geräte in diesem Profil blockiert wird. Außerhalb dieser Fenster gelten die normalen WAN-Zugriffsregeln des Profils. Ein „Kinder“-Profil könnte beispielsweise den WAN-Zugriff um 19 Uhr an Schultagen und um 21 Uhr am Wochenende blockieren, während ein „Admin“-Profil rund um die Uhr vollen Zugriff behält.

Dieser Abschnitt ist deaktiviert, wenn der WAN-Zugriff auf „Keiner“ gesetzt ist – das Internet ist bereits zu jeder Zeit blockiert, sodass es nichts zu planen gibt. Alle Fenster, die Sie konfiguriert haben, bleiben erhalten und werden wieder wirksam, wenn Sie den WAN-Zugriff erneut aktivieren.

### DNS

Wählen Sie, ob dieses Profil die DNS-Server des Systems oder eigene verwendet. Wählen Sie „Benutzerdefiniert“, um bis zu drei DNS-Server anzugeben, jeweils mit optionaler DoH-Verschlüsselung (DNS over HTTPS) für sichere Abfragen.`,
  '/profiles/blackout': `## Profile – Sperrzeiten

Eine Sperrzeit blockiert während des angegebenen Zeitraums den WAN-Zugriff (Internet) für alle Geräte in diesem Profil. Außerhalb der Sperrzeiten gelten die normalen WAN-Zugriffsregeln des Profils.

### Zeitfenster

Die Start- und Endzeiten für die Sperrzeit. Der WAN-Zugriff wird zwischen diesen Zeiten an den ausgewählten Tagen blockiert. Die Endzeit muss später als die Startzeit sein.

### Tage

Wählen Sie aus, an welchen Wochentagen die Sperrzeit aktiv sein soll.`,
  '/profiles/schedule': `### Sperrzeitplan

Festgelegte Zeiten, zu denen der WAN-Zugriff (Internet) für Geräte in diesem Profil blockiert wird. Außerhalb dieser Fenster gelten die normalen WAN-Zugriffsregeln des Profils. Ein „Kinder“-Profil könnte beispielsweise den WAN-Zugriff um 19 Uhr an Schultagen und um 21 Uhr am Wochenende blockieren, während ein „Admin“-Profil rund um die Uhr vollen Zugriff behält.

Klicken Sie auf +, um ein Fenster hinzuzufügen, doppelklicken Sie auf ein vorhandenes Fenster, um es zu bearbeiten oder zu löschen, oder ziehen Sie an seinen Griffen, um seine Größe direkt anzupassen.`,
  '/published-ports': `## Veröffentlichte Ports

### Veröffentlichte Ports

Machen Sie ausgewählte Ports eines Geräts aus dem Internet erreichbar. Datenverkehr aus dem Internet am angegebenen Port wird an das ausgewählte Gerät weitergeleitet.

### Bezeichnung

Ein aussagekräftiger Name, der Ihnen hilft, diese Regel zu identifizieren, etwa „Home Assistant“ oder „Minecraft-Server“.

### Gerät

Dem ausgewählten Gerät wird eine stabile IPv4-Adresse zugewiesen, um sicherzustellen, dass die Portweiterleitungsregel stets das vorgesehene Gerät erreicht. IPv6-Adressen sind über SLAAC in der Regel stabil.

### Port

Der Port oder Portbereich am Gerät, der freigegeben werden soll. Beispielsweise „443“ für einen einzelnen Port oder „27015-27030“ für einen Bereich.

### Protokoll

Wählen Sie TCP für die meisten Dienste (Web, SSH usw.), UDP für Gaming oder VoIP, oder beides, falls der Dienst es erfordert.

### Quelle

Beschränken Sie, wer sich verbinden darf, indem Sie eine IP-Adresse oder einen CIDR-Bereich angeben. Verwenden Sie „Beliebig“, um Verbindungen von überall im Internet zu erlauben.

### Endpunkte

Die öffentlichen Adressen, unter denen dieser Port erreichbar ist. IPv4-Endpunkte verwenden NAT, um Datenverkehr weiterzuleiten, während IPv6 die Firewall direkt zum Gerät öffnet.`,
  '/published-ports/dialog': `## Veröffentlichte Ports

Machen Sie ausgewählte Ports eines Geräts aus dem Internet erreichbar. Datenverkehr aus dem Internet am angegebenen Port wird an das ausgewählte Gerät weitergeleitet.

### Bezeichnung

Ein aussagekräftiger Name, der Ihnen hilft, diese Regel zu identifizieren, etwa „Home Assistant“ oder „Minecraft-Server“.

### Gerät

Das Gerät, an das der Datenverkehr weitergeleitet werden soll. Dem ausgewählten Gerät wird eine stabile IPv4-Adresse zugewiesen, um sicherzustellen, dass die Regel stets das vorgesehene Gerät erreicht.

### Port

Der Port oder Portbereich am Gerät, der freigegeben werden soll. Beispielsweise „443“ für einen einzelnen Port oder „27015-27030“ für einen Bereich.

### Protokoll

Wählen Sie TCP für die meisten Dienste (Web, SSH usw.), UDP für Gaming oder VoIP, oder beides, falls der Dienst es erfordert.

### Quelle

Beschränken Sie, wer sich verbinden darf, indem Sie eine IP-Adresse oder einen CIDR-Bereich angeben. Verwenden Sie „Beliebig“, um Verbindungen von überall im Internet zu erlauben.

### IP-Version

Wählen Sie, über welche IP-Versionen veröffentlicht werden soll. IPv4 verwendet NAT, um Datenverkehr weiterzuleiten, während IPv6 die Firewall direkt zum Gerät öffnet.

### Externer Port

Die für die Außenwelt sichtbare Portnummer. Verwenden Sie „Wie am Gerät“, um sie identisch zu halten, oder geben Sie einen anderen Port für den externen Zugriff an.`,
  '/wan/ipv4': `## WAN – IPv4

### IP-Adresse

-   **DHCP:** Ihr ISP weist Ihrem Router automatisch eine IP-Adresse zu. Dies ist die gebräuchlichste Konfiguration.
-   **PPPoE:** Wird von einigen DSL-Anbietern verwendet. Erfordert einen Benutzernamen und ein Passwort von Ihrem ISP.
-   **Static:** Konfigurieren Sie manuell eine feste IP-Adresse. Nur verwenden, wenn Ihr ISP Ihnen eine statische IP zugewiesen hat.`,
  '/wan/ipv6': `## WAN – IPv6

### IP-Adresse

-   **SLAAC:** Automatische IPv6-Konfiguration. Die gebräuchlichste Option, wenn Ihr ISP IPv6 unterstützt.
-   **DHCPv6:** Der ISP weist die IPv6-Adresse über DHCP zu. Verwenden, wenn SLAAC bei Ihrem ISP nicht funktioniert.
-   **Static:** Konfigurieren Sie manuell eine feste, von Ihrem ISP zugewiesene IPv6-Adresse.
-   **6RD:** Tunnelt IPv6 über eine IPv4-Verbindung. Erfordert Konfigurationsdetails von Ihrem ISP.
-   **Deaktiviert:** Deaktiviert IPv6 auf der WAN-Schnittstelle.

### IPv6-Präfix

Bei SLAAC und DHCPv6 gibt das Präfix die angeforderte Präfixlänge für die Präfixdelegierung an (z. B. /48, /56, /64). Lassen Sie das Feld leer, damit Ihr ISP automatisch entscheidet.

### DNS

DNS (Domain Name System) übersetzt Domänennamen in IP-Adressen.

-   **Vom ISP beziehen:** Verwenden Sie die von Ihrem ISP automatisch bereitgestellten DNS-Server.
-   **Benutzerdefiniert:** Geben Sie Ihre eigenen DNS-Server an. Es werden sowohl IPv4- als auch IPv6-Adressen unterstützt.`,
  '/wan/mac-address': `## WAN – MAC-Adresse

### MAC-Adresse

Die MAC-Adresse identifiziert Ihren Router gegenüber Ihrem ISP. Manche ISPs binden den Dienst an eine bestimmte MAC-Adresse.

-   **Router:** Verwenden Sie die integrierte MAC-Adresse des Routers.
-   **Benutzerdefiniert:** Klonen Sie die MAC-Adresse eines früheren Geräts. Nützlich, wenn Ihr ISP den Dienst an die MAC-Adresse Ihres alten Routers oder Modems gebunden hat.`,
  '/wan/dns': `## WAN – DNS

### DNS

DNS (Domain Name System) übersetzt Domänennamen in IP-Adressen.

-   **Vom ISP beziehen:** Verwenden Sie die von Ihrem ISP automatisch bereitgestellten DNS-Server.
-   **Benutzerdefiniert:** Geben Sie Ihre eigenen DNS-Server an (z. B. 1.1.1.1, 8.8.8.8).

### Sicher (DoH)

Aktivieren Sie DNS over HTTPS (DoH) für verschlüsselte DNS-Abfragen. Dies verbessert die Privatsphäre, indem es das Abhören Ihres DNS-Datenverkehrs verhindert.

**Hinweis:** Nicht alle DNS-Server unterstützen DoH. Gängige Server, die dies tun, sind Cloudflare (1.1.1.1), Google (8.8.8.8) und Quad9 (9.9.9.9).`,
  '/wan/dynamic-dns': `## WAN – Dynamisches DNS

### Dynamisches DNS

DDNS ordnet Ihre wechselnde IP-Adresse einem festen Domänennamen zu, sodass Sie aus der Ferne auf Ihr Netzwerk zugreifen können, ohne Ihre aktuelle IP zu kennen.

-   **Start9:** Kostenloser DDNS-Dienst von Start9. Keine zusätzliche Einrichtung erforderlich.
-   **Andere Anbieter:** Erfordert ein Konto beim Anbieter. Geben Sie die Zugangsdaten und den Hostnamen Ihres DDNS-Anbieters ein.`,
  '/settings/activity': `## Einstellungen – Aktivität

Zeigt aktuelle Anmeldeversuche und Sicherheitsereignisse an. Überwachen Sie verdächtige Aktivitäten und entfernen Sie bei Bedarf alte Einträge.`,
  '/settings/advanced': `## Einstellungen – Protokolle

Erweiterte Optionen für fortgeschrittene Benutzer. LuCI bietet direkten Zugriff auf die OpenWRT-Konfiguration. Support-Diagnosen helfen bei der Fehlerbehebung. Das Zurücksetzen auf Werkseinstellungen stellt die Standardeinstellungen wieder her.`,
  '/settings/backup': `## Einstellungen – Sicherung

### Sicherung erstellen

Laden Sie eine Sicherung aller Router-Einstellungen herunter, einschließlich Sicherheitsprofilen, WiFi-Konfiguration, SSL-Zertifikaten und Systemeinstellungen. Bewahren Sie die Sicherungsdatei an einem sicheren Ort auf.

### Sicherung wiederherstellen

Laden Sie eine zuvor erstellte Sicherungsdatei hoch, um alle Einstellungen wiederherzustellen. Der Router wird nach der Wiederherstellung neu gestartet, und alle aktuellen Einstellungen werden überschrieben.`,
  '/settings/general': `## Einstellungen – Allgemein

<details><summary>Einstellungen</summary> Allgemeine Einstellungen für die Benutzeroberfläche und das Verhalten des Routers.<h3>Design</h3>Wählen Sie zwischen dunklem oder hellem Design, oder behalten Sie die für die Benutzeroberfläche übernommenen Systemeinstellungen bei.<h3>Sprache</h3>Konfigurieren Sie die bevorzugte Sprache für die Benutzeroberfläche des Routers. Diese Einstellung beeinflusst die für alle Menüs, Optionen und sonstigen Textelemente angezeigte Sprache innerhalb der Weboberfläche des Routers.<h3>Zeitzone</h3>Legen Sie die lokale Zeitzone für den Router fest. Dies beeinflusst geplante Ereignisse wie WAN-Sperrzeiten und WiFi-Sperrzeitpläne. </details> <details><summary>Fernzugriff</summary> Ermöglicht den Fernzugriff zur Verwaltung der Einstellungen und Konfigurationen des Routers von außerhalb des lokalen Netzwerks.<ul><li><b>Wenn hinter NAT</b> (Network Address Translation): Erlaubt den Fernzugriff nur, wenn der Router Teil eines privaten Netzwerks ist, das NAT verwendet. NAT wird häufig in Heim- und Kleinunternehmensnetzwerken eingesetzt, in denen sich mehrere Geräte eine einzige öffentliche IP-Adresse teilen.</li><li><b>Nie:</b> Deaktiviert den Fernzugriff vollständig. Geeignet für Umgebungen, in denen Sicherheit oberste Priorität hat, etwa in hochsensiblen oder isolierten Netzwerken.</li><li><b>Immer:</b> Aktiviert den Fernzugriff jederzeit. Ideal für Umgebungen, in denen kontinuierliche Fernverwaltung erforderlich ist, etwa in einem Unternehmensnetzwerk, in dem IT-Administratoren ständigen Zugriff auf den Router für Überwachung und Aktualisierungen benötigen.</li></ul></details><details><summary>Sicherheit</summary> Laden Sie Ihr Root-CA-Zertifikat herunter, um HTTPS-Verbindungen von weiteren Geräten zu vertrauen. Durch die Installation dieses Zertifikats können Browser und Apps die Identität des Routers ohne Sicherheitswarnungen überprüfen. </details> <details><summary>Aktualisierungen</summary> Wenn eine Aktualisierung verfügbar ist, erscheint oben auf dieser Seite ein Banner. Sie können vor dem Aktualisieren die Versionshinweise einsehen. Der Router ist während des Aktualisierungsvorgangs kurzzeitig nicht erreichbar.</details>`,
  '/settings/logs': `## Einstellungen – Protokolle

Protokolle zeichnen Systemereignisse auf und helfen bei der Diagnose von Problemen. Nützlich für die Fehlerbehebung und Support-Anfragen.`,
  '/settings/password': `## Einstellungen – Passwort

Ändert das Administrator-Passwort für den Router. Erhöht die Sicherheit, indem es die Router-Einstellungen schützt und unbefugten Zugriff verhindert. Legen Sie ein starkes, eindeutiges Passwort fest.`,
  '/settings/ssh-keys': `## Einstellungen – SSH-Schlüssel

SSH-Schlüssel ermöglichen eine sichere, passwortlose Authentifizierung an Ihrem Router. Fügen Sie öffentliche Schlüssel für Benutzer hinzu, die SSH-Zugriff benötigen.`,
  '/settings/ssh-keys/dialog': `## Einstellungen – SSH-Schlüssel

SSH-Schlüssel ermöglichen eine sichere, passwortlose Authentifizierung an Ihrem Router. Fügen Sie den öffentlichen Schlüssel für einen Benutzer hinzu, der SSH-Zugriff benötigt.

### Öffentlicher Schlüssel

Fügen Sie die vollständige öffentliche Schlüsselzeichenfolge ein, einschließlich des Algorithmus-Präfixes und der Schlüsseldaten. Unterstützte Formate sind ssh-ed25519, ssh-rsa und ecdsa-sha2-nistp256/384/521.`,
  '/wifi/blackout-schedule': `## Wi-Fi – Sperrzeitplan

Legt festgelegte Zeiten fest, zu denen WiFi deaktiviert wird, sodass sich keine Geräte damit verbinden können. Diese Funktion wird häufig verwendet, um den Internetzugriff während bestimmter Zeiträume für verschiedene Zwecke einzuschränken, etwa um Ablenkungen zu reduzieren, Schlafenszeiten durchzusetzen oder die Netzwerksicherheit außerhalb der Nutzungszeiten zu erhöhen.

**Doppelklicken Sie auf vorhandene Intervalle, um sie zu bearbeiten oder zu löschen**

Ziehen Sie vorhandene Intervalle, um sie direkt zu bearbeiten.`,
  '/wifi/blackout-schedule/dialog': `## Wi-Fi – Sperrzeitplan

Eine Sperrzeit deaktiviert WiFi während des angegebenen Zeitraums und verhindert, dass sich Geräte verbinden. Nützlich, um Ablenkungen zu reduzieren, Schlafenszeiten durchzusetzen oder die Netzwerksicherheit außerhalb der Nutzungszeiten zu erhöhen.

### Zeitfenster

Die Start- und Endzeiten für die Sperrzeit. WiFi wird zwischen diesen Zeiten an den ausgewählten Tagen deaktiviert. Die Endzeit muss später als die Startzeit sein.

### Tage

Wählen Sie aus, an welchen Wochentagen die Sperrzeit aktiv sein soll.`,
  '/wifi/passwords': `## Wi-Fi – Passwörter

Verwalten Sie die Passwörter für drahtlose Netzwerke. Jeder Eintrag steht für ein Wi-Fi-Netzwerk mit eigenem Passwort und eigenem Sicherheitsprofil, das die Zugriffsberechtigungen steuert.

### Sicherheitsprofil

Steuert, worauf das verbundene Gerät im Netzwerk zugreifen kann. Weisen Sie ein Sicherheitsprofil zu, um den Zugriff auf bestimmte Ressourcen einzuschränken oder zu gewähren.`,
  '/wifi/passwords/dialog': `## Wi-Fi – Passwörter

Jeder Eintrag steht für ein Wi-Fi-Netzwerk mit eigenem Passwort und eigenem Sicherheitsprofil, das die Zugriffsberechtigungen steuert.

### Bezeichnung

Ein aussagekräftiger Name für dieses Wi-Fi-Netzwerk, etwa „Zuhause“ oder „Gastnetzwerk“. Dies ist der Netzwerkname (SSID), den Geräte sehen.

### Passwort

Das Passwort, das Geräte zum Verbinden verwenden. Muss mindestens 8 Zeichen lang sein. Verwenden Sie die Schaltfläche zum Generieren, um ein starkes, zufälliges Passwort zu erstellen.

### Sicherheitsprofil

Steuert, worauf das verbundene Gerät im Netzwerk zugreifen kann. Weisen Sie ein Sicherheitsprofil zu, um den Zugriff auf bestimmte Ressourcen einzuschränken oder zu gewähren.`,
  '/wifi/settings': `## Wi-Fi – Einstellungen

### Wi-Fi aktivieren

Schalten Sie das Funkmodul ein oder aus. Wenn deaktiviert, können sich keine Geräte über Wi-Fi verbinden.

### SSID

Die SSID (Service Set Identifier) ist der Name Ihres WiFi-Netzwerks. Sie identifiziert Ihr Netzwerk gegenüber Geräten, sodass diese das Netzwerk finden und sich damit verbinden können.

### Übertragung

Das Übertragen der SSID macht das Netzwerk für Geräte auffindbar, die nach drahtlosen Verbindungen suchen. Das Deaktivieren der Übertragung verbirgt das Netzwerk vor gelegentlichen Nutzern und fügt eine zusätzliche Sicherheitsebene hinzu.

### Frequenzband

Das Frequenzband, auf dem Ihr WiFi betrieben wird. 2,4 GHz bietet eine größere Reichweite, während 5 GHz höhere Geschwindigkeiten bietet. Wählen Sie „Beide“, um den Dualband-Betrieb zu aktivieren.

### Separat übertragen

Bei Verwendung beider Frequenzbänder erstellt diese Option separate SSIDs für 2,4 GHz und 5 GHz (z. B. „MeinNetzwerk“ und „MeinNetzwerk-5G“). Nützlich, wenn Sie ausdrücklich steuern möchten, mit welchem Band sich ein Gerät verbindet.

### Kanäle

Bestimmte Kanäle innerhalb jedes Frequenzbands. Wählen Sie „Auto“, damit der Router den besten Kanal auswählt, oder wählen Sie einen bestimmten Kanal, um Störungen durch benachbarte Netzwerke zu vermeiden.`,
}

export default HELP_DE
