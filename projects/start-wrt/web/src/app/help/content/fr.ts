// prettier-ignore
/** French help content (lazy-loaded on language switch). */
const HELP_FR: Record<string, string> = {
  '/devices': `## Appareils

Consultez et gérez tous les appareils de votre réseau. Les appareils en ligne sont actuellement connectés, les appareils hors ligne se sont déjà connectés mais ne sont pas actifs pour le moment.

### Nom

Le nom d’hôte de l’appareil ou un nom personnalisé que vous lui avez attribué. Cliquez pour afficher les détails et configurer l’appareil.

### Connexion

La manière dont l’appareil se connecte à votre réseau (Ethernet, Wi-Fi).

### Adresse MAC

L’identifiant matériel unique de l’interface réseau de l’appareil.

### Adresse IP

Les adresses IPv4 et IPv6 attribuées à l’appareil.

### Données & débit

L’utilisation de la bande passante et les débits de transfert actuels. Utile pour identifier les gros consommateurs du réseau.`,
  '/devices/device': `## Appareils

Consultez les informations de l’appareil et configurez ses paramètres.

### Résumé

Affiche l’état actuel de l’appareil, son type de connexion, son profil de sécurité et ses adresses IP. Une icône de cadenas indique une adresse IP réservée.

### Utilisation des données

Affiche l’historique d’utilisation réseau de cet appareil. Utilisez le menu déroulant pour consulter différentes périodes. Le téléchargement est indiqué en bleu, l’envoi en vert.

### Nom

Attribuez un nom personnalisé pour identifier facilement cet appareil. Si le champ est laissé vide, le nom d’hôte de l’appareil sera utilisé. L’espace réservé indique le nom qui sera utilisé par défaut.

### IP réservée

Réservez une adresse IP pour attribuer une adresse IPv4 fixe qui ne changera pas entre les redémarrages. Utile pour les serveurs, imprimantes, appareils NAS ou tout ce que vous devez joindre par une adresse IP constante.

### Oublier

Retire un appareil hors ligne de votre liste d’appareils. Tout nom personnalisé ou paramètre d’IP réservée sera perdu. Si l’appareil se reconnecte, il apparaîtra comme un nouvel appareil.`,
  '/ethernet': `## Ethernet

### Port

L’identifiant du port Ethernet physique (par ex. eth0, eth1).

### Profil de sécurité

Le profil de sécurité par défaut attribué aux appareils connectés via le port Ethernet spécifié.

### WAN

Le port WAN (Wide Area Network) se connecte au modem de votre ISP ou au réseau en amont. Un seul port Ethernet peut être désigné comme WAN à la fois.

### Changer le port WAN

Utilisez le bouton « Changer le port WAN » pour réattribuer la désignation WAN à un autre port Ethernet. Cela interrompra votre connexion Internet et redémarrera le routeur. Assurez-vous que votre modem est branché sur le nouveau port WAN avant de confirmer.`,
  '/ethernet/dialog': `## Changer le port WAN

Sélectionnez un nouveau port Ethernet pour servir de connexion WAN (Internet). Le port WAN actuel est présélectionné.

Changer le port WAN interrompra immédiatement votre connexion Internet et redémarrera le routeur. Assurez-vous que votre modem ou le câble du réseau en amont est branché sur le nouveau port avant de confirmer.`,
  '/inbound': `## VPN entrants

Les serveurs VPN sont des applications logicielles qui fournissent un accès sécurisé et chiffré aux ressources du réseau interne, permettant aux clients d’établir une connexion entrante vers le réseau en toute sécurité via Internet depuis n’importe où dans le monde.

### Profil de sécurité

Affiche les autorisations d’accès définies pour l’appareil. Gère et contrôle ce à quoi l’appareil peut accéder sur le réseau via un profil de sécurité ou un planning.

### Port

Le port réseau sur lequel le serveur VPN écoute les connexions entrantes.`,
  '/inbound/dialog': `## VPN entrants

### Étiquette

Un nom clair et descriptif pour le serveur VPN, tel que « Serveur VPN du bureau » ou « Serveur VPN domestique », afin de le distinguer des autres connexions VPN.

### Point de terminaison

L’adresse IP ou le nom de domaine où le serveur VPN peut être joint par les clients se connectant via Internet.

### Profil de sécurité

L’ensemble initial de droits d’accès et de restrictions attribué aux clients lorsqu’ils se connectent au serveur VPN. Sélectionnez un profil ou un planning déjà créé, ou créez-en un nouveau.

### Port

Le port réseau sur lequel le serveur VPN écoute les connexions entrantes. Le port couramment utilisé pour WireGuard est 51820.`,
  '/inbound/client': `## VPN entrants

Gérez les appareils clients de ce serveur VPN.

### Nom

Un nom choisi pour identifier facilement la fonction de l’appareil client.

### Adresse IP LAN

L’adresse IP attribuée sur le réseau local. Elle a été configurée lors de la mise en place de l’appareil client.

### Acheminement

Affiche le mode d’acheminement du trafic du client. « Tout le trafic » achemine tout le trafic Internet par le tunnel VPN. « LAN uniquement » achemine seulement le trafic du réseau local par le tunnel. Utilisez le menu d’actions pour basculer entre les modes.`,
  '/inbound/client/dialog-add': `## VPN entrants

### Étiquette

Un nom clair et descriptif pour l’appareil client, tel que « Portable de Jean » ou « Téléphone du bureau », afin de le distinguer des autres appareils.

### Adresse IP LAN

L’adresse IP LAN souhaitée pour l’appareil client, en veillant à ce qu’elle se situe dans la plage d’IP allouée au VPN. Garantit que l’appareil client dispose d’une adresse IP unique au sein du réseau VPN, lui permettant de communiquer avec les autres appareils.

### Clé publique

La clé publique WireGuard de l’appareil client. Si le champ est laissé vide, une paire de clés sera générée automatiquement. Saisissez une clé publique existante si l’appareil client en a déjà une configurée.

### Acheminer tout le trafic par le tunnel

Lorsque cette option est activée, tout le trafic Internet du client est acheminé par le tunnel VPN. Lorsqu’elle est désactivée (valeur par défaut), seul le trafic destiné au réseau local passe par le tunnel et le client utilise sa propre connexion Internet pour tout le reste.`,
  '/inbound/client/dialog-config': `## Configuration du client

La configuration WireGuard de cet appareil client. Utilisez-la pour configurer la connexion VPN sur le client.

### Fichier

Affiche la configuration sous forme de texte. Utilisez le bouton de copie pour la copier dans votre presse-papiers, ou le bouton de téléchargement pour l’enregistrer sous forme de fichier <code>wireguard.conf</code> pouvant être importé directement dans l’application WireGuard.

### QR

Affiche la configuration sous forme de QR code. Scannez-le avec l’application mobile WireGuard pour configurer l’appareil client sans saisie manuelle.`,
  '/inbound/client/dialog-rename': `## Renommer le client

### Nom

Saisissez un nouveau nom pour cet appareil client VPN. Utilisez un nom descriptif, tel que « Portable de Jean » ou « Téléphone du bureau », pour identifier facilement l’appareil dans votre liste de clients.`,
  '/lan/ipv4': `## LAN – IPv4

### Bloc réseau

Le bloc d’IP privées /16 de votre réseau. Chaque profil de sécurité recevra son propre sous-réseau /24 au sein de ce bloc, permettant jusqu’à 256 sous-réseaux distincts de 254 appareils chacun.

### IP du routeur

L’IP LAN du routeur est la passerelle (.1) du sous-réseau du profil de sécurité Admin. Les deux premiers octets proviennent du bloc réseau ci-dessus ; le troisième octet est défini par le champ Sous-réseau du profil Admin, pas sur cette page.`,
  '/lan/ipv6': `## LAN – IPv6

### IPv6

Lorsque cette option est activée, les appareils de votre réseau recevront des adresses IPv6 via SLAAC (Stateless Address Autoconfiguration). C’est la méthode standard d’attribution d’adresses IPv6.

IPv6 permet à vos appareils d’être directement joignables depuis Internet sans NAT, ce qui est utile pour héberger des services ou pour les applications pair-à-pair.

### Longueur du préfixe

La longueur du préfixe détermine la taille de l’espace d’adressage IPv6 de votre LAN. Elle doit être **plus grande** (un nombre plus élevé) que votre préfixe WAN pour créer un sous-réseau valide.

Par exemple, si votre ISP vous attribue un préfixe /48, vous pouvez utiliser /56, /60 ou /64 pour votre LAN. Un préfixe /64 est recommandé pour la plupart des réseaux domestiques.`,
  '/outbound': `## VPN sortants

Les VPN sortants acheminent votre trafic Internet par des tunnels chiffrés vers des serveurs distants, renforçant la confidentialité et la sécurité.

### Pourquoi utiliser des VPN sortants ?

-   Masquer votre adresse IP des sites web et des services
-   Chiffrer le trafic sur les réseaux non fiables
-   Accéder à du contenu soumis à des restrictions géographiques
-   Empêcher le pistage par l’ISP

### Chaînage de VPN

Acheminez un VPN par un autre pour davantage de confidentialité. Votre trafic ressort par plusieurs serveurs, ce qui le rend plus difficile à retracer jusqu’à vous. Notez que le chaînage augmente la latence.

### Pour commencer

Téléversez un fichier de configuration WireGuard fourni par votre fournisseur de VPN (Mullvad, ProtonVPN, IVPN, etc.) pour créer un nouveau client VPN.`,
  '/outbound/vpn': `## VPN sortants

<details><summary>Étiquette</summary> Un nom convivial pour identifier cette connexion VPN. Utilisez un nom descriptif comme « Mullvad Suède » ou « VPN du travail ». </details> <details><summary>Se connecte à</summary> L’endroit où le trafic de ce VPN doit être acheminé :<ul><li><b>Internet :</b> Le trafic ressort directement vers Internet par ce VPN.</li><li><b>Un autre VPN :</b> Chaîner d’abord par un autre VPN pour plus de confidentialité. Votre trafic sera chiffré plusieurs fois et ressortira par plusieurs serveurs.</li></ul><b>Remarque :</b> Le chaînage de VPN augmente la latence et peut réduire les débits. </details> <details><summary>Chemin de connexion</summary> Affiche l’itinéraire complet que prend votre trafic depuis ce VPN jusqu’à Internet. Par exemple : Mullvad → Proton → Internet signifie que le trafic est chiffré par Mullvad, envoyé via Proton, puis ressort vers Internet.</details>`,
  '/outbound/dialog': `## VPN sortants

Les clients VPN établissent une connexion sortante vers un serveur VPN, permettant aux appareils d’envoyer et de recevoir des données comme s’ils étaient directement connectés au réseau privé. Masque votre adresse IP, contourne les restrictions géographiques et protège les données sensibles sur les réseaux Wi-Fi publics.

### Étiquette

Un nom clair et descriptif pour la connexion VPN, tel que « Mullvad Suède » ou « VPN du travail », pour la distinguer facilement des autres connexions.

### Fichier de configuration

Téléversez un fichier **.conf** WireGuard fourni par votre fournisseur de VPN. Ce fichier contient l’adresse du serveur, les clés et les paramètres de connexion nécessaires pour établir le tunnel. La plupart des fournisseurs (Mullvad, ProtonVPN, IVPN, etc.) proposent des fichiers de configuration WireGuard à télécharger depuis leur site web ou leur application.

### Cible

Le saut suivant dans le chemin d’une connexion VPN en couches.`,
  '/profiles': `## Profils

Les profils de sécurité définissent des segments de réseau aux autorisations isolées. Chaque profil contrôle l’accès à Internet et la communication avec les autres profils de votre réseau local.

### Nom

Un nom descriptif pour le profil de sécurité, tel que « Invité » ou « Appareils IoT ».

### DNS

Les serveurs DNS utilisés par les appareils de ce profil — ceux du système (« Système »), les serveurs personnalisés du profil (« Personnalisé »), ou, lors de l’acheminement par un VPN, ceux du VPN (« VPN »).

### Sortant

La manière dont le trafic de ce profil atteint Internet — directement (« Direct »), ou par l’un de vos clients VPN (indiqué par son nom).

### Accès LAN

Les autres profils avec lesquels ce profil peut communiquer sur le réseau local — tous, uniquement le sien, ou une liste blanche choisie.

### Accès WAN

Indique si les appareils de ce profil ont accès à Internet, éventuellement restreint par une liste blanche ou une liste noire d’adresses IP ou de plages CIDR.`,
  '/profiles/dialog': `## Profils

Les profils de sécurité définissent des segments de réseau aux autorisations isolées. Chaque profil contrôle l’accès à Internet et la communication avec les autres profils de votre réseau local.

### Nom

Un nom descriptif pour le profil de sécurité, tel que « Invité » ou « Appareils IoT ».

### Sous-réseau

Le troisième octet du sous-réseau IPv4 de ce profil. Par exemple, une valeur de 2 crée le sous-réseau 192.168.2.0/24. Chaque profil doit avoir un sous-réseau unique.

### Accès LAN

Contrôle avec quels autres profils ce profil peut communiquer sur le réseau local. Choisissez « Tous » pour un accès complet, « Même profil » pour l’isolation, ou « Liste blanche » pour sélectionner des profils individuels.

### Ajouter automatiquement les nouveaux profils à la liste blanche

Lorsque l’accès LAN est réglé sur « Liste blanche », activer cette option accorde également l’accès à tous les profils créés à l’avenir — utile pour les profils d’administration qui doivent atteindre tous les segments du réseau.

### Acheminement sortant

Choisissez comment le trafic de ce profil atteint Internet. Sélectionnez « Direct » pour un accès Internet normal, ou « VPN » pour acheminer tout le trafic du profil par l’un de vos clients VPN.

### Accès WAN

Indique si les appareils de ce profil ont accès à Internet. Utilisez « Liste blanche » ou « Liste noire » pour restreindre l’accès à des adresses IP ou des plages CIDR spécifiques, ou « Aucun » pour bloquer entièrement Internet.

### Planning d’extinction

Plages désignées durant lesquelles l’accès WAN (Internet) est bloqué pour les appareils de ce profil. En dehors de ces plages, les règles d’accès WAN normales du profil s’appliquent. Par exemple, un profil « Enfants » pourrait bloquer l’accès WAN à 19 h les soirs d’école et à 21 h le week-end, tandis qu’un profil « Admin » conserve un accès complet 24 h/24 et 7 j/7.

Cette section est désactivée lorsque l’accès WAN est réglé sur « Aucun » — Internet est déjà bloqué en permanence, il n’y a donc rien à planifier. Les plages que vous avez configurées sont conservées et reprennent effet si vous réactivez l’accès WAN.

### DNS

Choisissez si ce profil utilise les serveurs DNS du système ou les siens. Sélectionnez « Personnalisé » pour spécifier jusqu’à trois serveurs DNS, chacun avec un chiffrement DoH (DNS over HTTPS) facultatif pour des résolutions sécurisées.`,
  '/profiles/blackout': `## Profils – Plages d’extinction

Une plage d’extinction bloque l’accès WAN (Internet) pour tous les appareils de ce profil pendant la période spécifiée. Les règles d’accès WAN normales du profil s’appliquent en dehors des plages d’extinction.

### Plage horaire

Les heures de début et de fin de la période d’extinction. L’accès WAN sera bloqué entre ces heures les jours sélectionnés. L’heure de fin doit être postérieure à l’heure de début.

### Jours

Sélectionnez les jours de la semaine où la plage d’extinction doit être active.`,
  '/profiles/schedule': `### Planning d’extinction

Plages désignées durant lesquelles l’accès WAN (Internet) est bloqué pour les appareils de ce profil. En dehors de ces plages, les règles d’accès WAN normales du profil s’appliquent. Par exemple, un profil « Enfants » pourrait bloquer l’accès WAN à 19 h les soirs d’école et à 21 h le week-end, tandis qu’un profil « Admin » conserve un accès complet 24 h/24 et 7 j/7.

Cliquez sur + pour ajouter une plage, double-cliquez sur une plage existante pour la modifier ou la supprimer, ou faites glisser ses poignées pour la redimensionner sur place.`,
  '/published-ports': `## Ports publiés

### Ports publiés

Rendez certains ports d’un appareil joignables depuis Internet. Le trafic provenant d’Internet sur le port spécifié sera transféré vers l’appareil sélectionné.

### Étiquette

Un nom descriptif pour vous aider à identifier cette règle, tel que « Home Assistant » ou « Serveur Minecraft ».

### Appareil

L’appareil que vous sélectionnez se verra attribuer une adresse IPv4 stable pour garantir que la règle de transfert de port atteint toujours l’appareil voulu. Les adresses IPv6 sont généralement stables grâce à SLAAC.

### Port

Le port ou la plage de ports à exposer sur l’appareil. Par exemple, « 443 » pour un seul port ou « 27015-27030 » pour une plage.

### Protocole

Choisissez TCP pour la plupart des services (web, SSH, etc.), UDP pour les jeux ou la VoIP, ou les deux si le service l’exige.

### Source

Limitez qui peut se connecter en spécifiant une adresse IP ou une plage CIDR. Utilisez « N’importe laquelle » pour autoriser les connexions depuis n’importe où sur Internet.

### Points de terminaison

Les adresses publiques où ce port peut être joint. Les points de terminaison IPv4 utilisent le NAT pour transférer le trafic, tandis qu’IPv6 ouvre le pare-feu directement vers l’appareil.`,
  '/published-ports/dialog': `## Ports publiés

Rendez certains ports d’un appareil joignables depuis Internet. Le trafic provenant d’Internet sur le port spécifié sera transféré vers l’appareil sélectionné.

### Étiquette

Un nom descriptif pour vous aider à identifier cette règle, tel que « Home Assistant » ou « Serveur Minecraft ».

### Appareil

L’appareil vers lequel transférer le trafic. L’appareil sélectionné se verra attribuer une adresse IPv4 stable pour garantir que la règle atteint toujours l’appareil voulu.

### Port

Le port ou la plage de ports à exposer sur l’appareil. Par exemple, « 443 » pour un seul port ou « 27015-27030 » pour une plage.

### Protocole

Choisissez TCP pour la plupart des services (web, SSH, etc.), UDP pour les jeux ou la VoIP, ou les deux si le service l’exige.

### Source

Limitez qui peut se connecter en spécifiant une adresse IP ou une plage CIDR. Utilisez « N’importe laquelle » pour autoriser les connexions depuis n’importe où sur Internet.

### Version IP

Choisissez sur quelles versions d’IP publier. IPv4 utilise le NAT pour transférer le trafic, tandis qu’IPv6 ouvre le pare-feu directement vers l’appareil.

### Port externe

Le numéro de port visible depuis l’extérieur. Utilisez « Identique à l’appareil » pour le conserver identique, ou spécifiez un port différent pour l’accès externe.`,
  '/wan/ipv4': `## WAN – IPv4

### Adresse IP

-   **DHCP :** Votre ISP attribue automatiquement une adresse IP à votre routeur. C’est la configuration la plus courante.
-   **PPPoE :** Utilisé par certains fournisseurs DSL. Nécessite un nom d’utilisateur et un mot de passe fournis par votre ISP.
-   **Static :** Configurez manuellement une adresse IP fixe. À utiliser uniquement si votre ISP vous a attribué une IP statique.`,
  '/wan/ipv6': `## WAN – IPv6

### Adresse IP

-   **SLAAC :** Configuration IPv6 automatique. L’option la plus courante si votre ISP prend en charge IPv6.
-   **DHCPv6 :** L’ISP attribue une adresse IPv6 via DHCP. À utiliser si SLAAC ne fonctionne pas avec votre ISP.
-   **Static :** Configurez manuellement une adresse IPv6 fixe attribuée par votre ISP.
-   **6RD :** Encapsule IPv6 sur une connexion IPv4. Nécessite des détails de configuration fournis par votre ISP.
-   **Désactivé :** Désactive IPv6 sur l’interface WAN.

### Préfixe IPv6

Pour SLAAC et DHCPv6, le préfixe spécifie la longueur de préfixe demandée pour la délégation de préfixe (par ex. /48, /56, /64). Laissez vide pour laisser votre ISP décider automatiquement.

### DNS

Le DNS (Domain Name System) traduit les noms de domaine en adresses IP.

-   **Obtenir de l’ISP :** Utilisez les serveurs DNS fournis automatiquement par votre ISP.
-   **Personnalisé :** Spécifiez vos propres serveurs DNS. Les adresses IPv4 et IPv6 sont prises en charge.`,
  '/wan/mac-address': `## WAN – Adresse MAC

### Adresse MAC

L’adresse MAC identifie votre routeur auprès de votre ISP. Certains ISP verrouillent le service à une adresse MAC spécifique.

-   **Routeur :** Utilisez l’adresse MAC intégrée du routeur.
-   **Personnalisé :** Clonez l’adresse MAC d’un appareil précédent. Utile si votre ISP a verrouillé le service à l’adresse MAC de votre ancien routeur ou modem.`,
  '/wan/dns': `## WAN – DNS

### DNS

Le DNS (Domain Name System) traduit les noms de domaine en adresses IP.

-   **Obtenir de l’ISP :** Utilisez les serveurs DNS fournis automatiquement par votre ISP.
-   **Personnalisé :** Spécifiez vos propres serveurs DNS (par ex. 1.1.1.1, 8.8.8.8).

### Sécurisé (DoH)

Activez le DNS over HTTPS (DoH) pour des requêtes DNS chiffrées. Cela améliore la confidentialité en empêchant l’écoute de votre trafic DNS.

**Remarque :** Tous les serveurs DNS ne prennent pas en charge le DoH. Parmi les serveurs courants qui le font figurent Cloudflare (1.1.1.1), Google (8.8.8.8) et Quad9 (9.9.9.9).`,
  '/wan/dynamic-dns': `## WAN – DNS dynamique

### DNS dynamique

Le DDNS associe votre adresse IP changeante à un nom de domaine fixe, vous permettant d’accéder à votre réseau à distance sans connaître votre adresse IP actuelle.

-   **Start9 :** Service DDNS gratuit fourni par Start9. Aucune configuration supplémentaire requise.
-   **Autres fournisseurs :** Nécessite un compte chez le fournisseur. Saisissez les identifiants et le nom d’hôte de votre fournisseur DDNS.`,
  '/settings/activity': `## Paramètres – Activité

Consultez les tentatives de connexion récentes et les évènements de sécurité. Surveillez les activités suspectes et supprimez les anciennes entrées selon vos besoins.`,
  '/settings/advanced': `## Paramètres – Journaux

Options avancées pour les utilisateurs expérimentés. LuCI fournit un accès direct à la configuration d’OpenWRT. Les diagnostics d’assistance aident à résoudre les problèmes. La réinitialisation d’usine restaure les paramètres par défaut.`,
  '/settings/backup': `## Paramètres – Sauvegarde

### Créer une sauvegarde

Téléchargez une sauvegarde de tous les paramètres du routeur, y compris les profils de sécurité, la configuration Wi-Fi, les certificats SSL et les préférences système. Conservez le fichier de sauvegarde dans un endroit sûr.

### Restaurer une sauvegarde

Téléversez un fichier de sauvegarde précédemment créé pour restaurer tous les paramètres. Le routeur redémarrera après la restauration et tous les paramètres actuels seront écrasés.`,
  '/settings/general': `## Paramètres – Général

<details><summary>Préférences</summary> Préférences générales pour l’interface utilisateur et le comportement du routeur.<h3>Thème</h3>Choisissez un thème sombre ou clair, ou conservez les paramètres de votre système appliqués à l’interface.<h3>Langue</h3>Configurez la langue préférée pour l’interface utilisateur du routeur. Ce paramètre affecte la langue affichée pour tous les menus, options et autres éléments de texte de l’interface web du routeur.<h3>Fuseau horaire</h3>Définissez le fuseau horaire local du routeur. Cela affecte les évènements planifiés tels que les plages d’extinction WAN et les plannings d’extinction Wi-Fi. </details> <details><summary>Accès à distance</summary> Active l’accès à distance pour gérer les paramètres et configurations du routeur depuis l’extérieur du réseau local.<ul><li><b>Derrière un NAT</b> (Network Address Translation) : Autorise l’accès à distance uniquement lorsque le routeur fait partie d’un réseau privé utilisant le NAT. Le NAT est couramment utilisé dans les réseaux domestiques et des petites entreprises où plusieurs appareils partagent une seule adresse IP publique.</li><li><b>Jamais :</b> Désactive complètement l’accès à distance. Convient aux environnements où la sécurité est primordiale, comme les réseaux hautement sensibles ou isolés.</li><li><b>Toujours :</b> Active l’accès à distance en permanence. Idéal pour les environnements où une gestion à distance continue est nécessaire, comme un réseau d’entreprise où les administrateurs informatiques ont besoin d’un accès constant au routeur pour la surveillance et les mises à jour.</li></ul></details><details><summary>Sécurité</summary> Téléchargez votre certificat Root CA pour faire confiance aux connexions HTTPS depuis d’autres appareils. L’installation de ce certificat permet aux navigateurs et aux applications de vérifier l’identité du routeur sans avertissements de sécurité. </details> <details><summary>Mises à jour</summary> Lorsqu’une mise à jour est disponible, une bannière apparaît en haut de cette page. Vous pouvez consulter les notes de version avant de mettre à jour. Le routeur connaîtra une brève indisponibilité pendant le processus de mise à jour.</details>`,
  '/settings/logs': `## Paramètres – Journaux

Les journaux enregistrent les évènements système et aident à diagnostiquer les problèmes. Utiles pour le dépannage et les demandes d’assistance.`,
  '/settings/password': `## Paramètres – Mot de passe

Modifie le mot de passe administrateur du routeur. Renforce la sécurité en protégeant les paramètres du routeur et en empêchant les accès non autorisés. Définissez un mot de passe fort et unique.`,
  '/settings/ssh-keys': `## Paramètres – Clés SSH

Les clés SSH permettent une authentification sécurisée et sans mot de passe vers votre routeur. Ajoutez les clés publiques des utilisateurs ayant besoin d’un accès SSH.`,
  '/settings/ssh-keys/dialog': `## Paramètres – Clés SSH

Les clés SSH permettent une authentification sécurisée et sans mot de passe vers votre routeur. Ajoutez la clé publique d’un utilisateur ayant besoin d’un accès SSH.

### Clé publique

Collez la chaîne complète de la clé publique, y compris le préfixe de l’algorithme et les données de la clé. Les formats pris en charge incluent ssh-ed25519, ssh-rsa et ecdsa-sha2-nistp256/384/521.`,
  '/wifi/blackout-schedule': `## Wi-Fi – Planning d’extinction

Spécifie les plages désignées durant lesquelles le Wi-Fi sera désactivé, empêchant tout appareil de s’y connecter. Cette fonctionnalité est souvent utilisée pour restreindre l’accès à Internet pendant certaines périodes à diverses fins, telles que réduire les distractions, imposer des routines de coucher ou renforcer la sécurité du réseau pendant les heures creuses.

**Double-cliquez sur les plages existantes pour les modifier ou les supprimer**

Faites glisser les plages existantes pour les modifier sur place.`,
  '/wifi/blackout-schedule/dialog': `## Wi-Fi – Planning d’extinction

Une plage d’extinction désactive le Wi-Fi pendant la période spécifiée, empêchant tout appareil de se connecter. Utile pour réduire les distractions, imposer des routines de coucher ou renforcer la sécurité du réseau pendant les heures creuses.

### Plage horaire

Les heures de début et de fin de la période d’extinction. Le Wi-Fi sera désactivé entre ces heures les jours sélectionnés. L’heure de fin doit être postérieure à l’heure de début.

### Jours

Sélectionnez les jours de la semaine où la plage d’extinction doit être active.`,
  '/wifi/passwords': `## Wi-Fi – Mots de passe

Gérez les mots de passe des réseaux sans fil. Chaque entrée représente un réseau Wi-Fi avec son propre mot de passe et son profil de sécurité contrôlant les autorisations d’accès.

### Profil de sécurité

Contrôle ce à quoi l’appareil connecté peut accéder sur le réseau. Attribuez un profil de sécurité pour limiter ou accorder l’accès à des ressources spécifiques.`,
  '/wifi/passwords/dialog': `## Wi-Fi – Mots de passe

Chaque entrée représente un réseau Wi-Fi avec son propre mot de passe et son profil de sécurité contrôlant les autorisations d’accès.

### Étiquette

Un nom descriptif pour ce réseau Wi-Fi, tel que « Maison » ou « Réseau invité ». C’est le nom du réseau (SSID) que les appareils verront.

### Mot de passe

Le mot de passe que les appareils utiliseront pour se connecter. Doit comporter au moins 8 caractères. Utilisez le bouton de génération pour créer un mot de passe aléatoire fort.

### Profil de sécurité

Contrôle ce à quoi l’appareil connecté peut accéder sur le réseau. Attribuez un profil de sécurité pour limiter ou accorder l’accès à des ressources spécifiques.`,
  '/wifi/settings': `## Wi-Fi – Paramètres

### Activer le Wi-Fi

Allumez ou éteignez la radio sans fil. Lorsqu’elle est désactivée, aucun appareil ne peut se connecter en Wi-Fi.

### SSID

Le SSID (Service Set Identifier) est le nom de votre réseau Wi-Fi. Il identifie votre réseau auprès des appareils, leur permettant de le trouver et de s’y connecter.

### Diffusion

Diffuser le SSID rend le réseau détectable par les appareils recherchant des connexions sans fil. Désactiver la diffusion masque le réseau des utilisateurs occasionnels, ajoutant une couche de sécurité.

### Bande de fréquence

La bande de fréquence sur laquelle votre Wi-Fi fonctionne. La bande 2,4 GHz offre une meilleure portée, tandis que la bande 5 GHz offre des débits plus élevés. Choisissez « Les deux » pour activer le fonctionnement double bande.

### Diffuser séparément

Lors de l’utilisation des deux bandes de fréquence, cette option crée des SSID distincts pour 2,4 GHz et 5 GHz (par ex. « MonRéseau » et « MonRéseau-5G »). Utile si vous souhaitez contrôler explicitement à quelle bande un appareil se connecte.

### Canaux

Canaux spécifiques au sein de chaque bande de fréquence. Sélectionnez « Auto » pour que le routeur choisisse le meilleur canal, ou choisissez un canal spécifique pour éviter les interférences des réseaux voisins.`,
}

export default HELP_FR
