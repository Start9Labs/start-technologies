// prettier-ignore
/** Spanish help content (lazy-loaded on language switch). */
const HELP_ES: Record<string, string> = {
  '/devices': `## Dispositivos

Consulta y gestiona todos los dispositivos de tu red. Los dispositivos en línea están conectados actualmente; los dispositivos sin conexión se han conectado antes pero no están activos ahora.

### Nombre

El nombre de host del dispositivo o un nombre personalizado que le hayas asignado. Haz clic para ver los detalles y configurar el dispositivo.

### Conexión

Cómo se conecta el dispositivo a tu red (Ethernet, Wi-Fi).

### Dirección MAC

El identificador de hardware único de la interfaz de red del dispositivo.

### Dirección IP

Las direcciones IPv4 e IPv6 asignadas al dispositivo.

### Datos y velocidad

Uso de ancho de banda y velocidades de transferencia actuales. Útil para identificar a los usuarios que consumen más red.`,
  '/devices/device': `## Dispositivos

Consulta la información del dispositivo y configura sus ajustes.

### Resumen

Muestra el estado actual del dispositivo, el tipo de conexión, el perfil de seguridad y las direcciones IP. Un icono de candado indica una dirección IP reservada.

### Uso de datos

Muestra el uso histórico de la red para este dispositivo. Usa el desplegable para ver distintos periodos de tiempo. La descarga se muestra en azul y la subida en verde.

### Nombre

Asigna un nombre personalizado para identificar fácilmente este dispositivo. Si se deja vacío, se usará el nombre de host del dispositivo. El marcador de posición muestra el nombre que se aplicará por defecto.

### IP reservada

Reserva una dirección IP para asignar una dirección IPv4 fija que no cambiará entre reinicios. Útil para servidores, impresoras, dispositivos NAS o cualquier cosa a la que necesites acceder mediante una dirección IP constante.

### Olvidar

Elimina un dispositivo sin conexión de tu lista de dispositivos. Se perderá cualquier nombre personalizado o ajuste de IP reservada. Si el dispositivo vuelve a conectarse, aparecerá como un dispositivo nuevo.`,
  '/ethernet': `## Ethernet

### Puerto

El identificador del puerto físico Ethernet (p. ej., eth0, eth1).

### Perfil de seguridad

El perfil de seguridad por defecto asignado a los dispositivos conectados a través del puerto Ethernet especificado.

### WAN

El puerto WAN (Wide Area Network) se conecta al módem de tu ISP o a la red ascendente. Solo un puerto Ethernet puede designarse como WAN a la vez.

### Cambiar puerto WAN

Usa el botón «Cambiar puerto WAN» para reasignar la designación WAN a un puerto Ethernet distinto. Esto interrumpirá tu conexión a internet y reiniciará el router. Asegúrate de que tu módem esté conectado al nuevo puerto WAN antes de confirmar.`,
  '/ethernet/dialog': `## Cambiar puerto WAN

Selecciona un nuevo puerto Ethernet que sirva como conexión WAN (internet). El puerto WAN actual está preseleccionado.

Cambiar el puerto WAN interrumpirá de inmediato tu conexión a internet y reiniciará el router. Asegúrate de que tu módem o el cable de la red ascendente esté conectado al nuevo puerto antes de confirmar.`,
  '/inbound': `## VPN entrantes

Los servidores VPN son aplicaciones de software que proporcionan acceso seguro y cifrado a los recursos internos de la red, permitiendo a los clientes crear una conexión entrante a la red de forma segura a través de internet desde cualquier parte del mundo.

### Perfil de seguridad

Muestra los permisos de acceso establecidos para el dispositivo. Gestiona y controla a qué puede acceder el dispositivo en la red mediante un perfil de seguridad o un apagado.

### Puerto

El puerto de red en el que el servidor VPN escucha las conexiones entrantes.`,
  '/inbound/dialog': `## VPN entrantes

### Etiqueta

Un nombre claro y descriptivo para el servidor VPN, como «Servidor VPN de la oficina» o «Servidor VPN de casa», para diferenciarlo de otras conexiones VPN.

### Endpoint

La dirección IP o el nombre de dominio donde los clientes que se conectan a través de internet pueden alcanzar el servidor VPN.

### Perfil de seguridad

El conjunto inicial de derechos de acceso y restricciones asignados a los clientes cuando se conectan al servidor VPN. Selecciona un perfil o un apagado creado previamente, o crea uno nuevo.

### Puerto

El puerto de red en el que el servidor VPN escucha las conexiones entrantes. El puerto habitual usado para WireGuard es 51820.`,
  '/inbound/client': `## VPN entrantes

Gestiona los dispositivos cliente de este servidor VPN.

### Nombre

Un nombre elegido para identificar fácilmente la función del dispositivo cliente.

### Dirección IP LAN

La dirección IP asignada en la red de área local. Se configuró cuando se dio de alta el dispositivo cliente.

### Enrutamiento

Muestra el modo de enrutamiento del tráfico del cliente. «Todo el tráfico» enruta todo el tráfico de internet a través del túnel VPN. «Solo LAN» enruta únicamente el tráfico de la red local a través del túnel. Usa el menú de acciones para cambiar entre modos.`,
  '/inbound/client/dialog-add': `## VPN entrantes

### Etiqueta

Un nombre claro y descriptivo para el dispositivo cliente, como «Portátil de Juan» o «Teléfono de la oficina», para diferenciarlo de otros dispositivos.

### Dirección IP LAN

La dirección IP LAN deseada para el dispositivo cliente, asegurándote de que esté dentro del rango de IP asignado a la VPN. Garantiza que el dispositivo cliente tenga una dirección IP única dentro de la red VPN, permitiéndole comunicarse con otros dispositivos.

### Clave pública

La clave pública de WireGuard para el dispositivo cliente. Si se deja vacía, se generará automáticamente un par de claves. Introduce una clave pública existente si el dispositivo cliente ya tiene una configurada.

### Enrutar todo el tráfico por el túnel

Cuando está activado, todo el tráfico de internet del cliente se enruta a través del túnel VPN. Cuando está desactivado (la opción predeterminada), solo el tráfico destinado a la red local pasa por el túnel y el cliente usa su propia conexión a internet para todo lo demás.`,
  '/inbound/client/dialog-config': `## Configuración del cliente

La configuración de WireGuard para este dispositivo cliente. Úsala para establecer la conexión VPN en el cliente.

### Archivo

Muestra la configuración como texto. Usa el botón de copiar para copiarla al portapapeles, o el botón de descarga para guardarla como un archivo <code>wireguard.conf</code> que se puede importar directamente en la aplicación de WireGuard.

### QR

Muestra la configuración como un código QR. Escanéalo con la aplicación móvil de WireGuard para configurar el dispositivo cliente sin introducir los datos manualmente.`,
  '/inbound/client/dialog-rename': `## Renombrar cliente

### Nombre

Introduce un nuevo nombre para este dispositivo cliente VPN. Usa algo descriptivo, como «Portátil de Juan» o «Teléfono de la oficina», para identificar fácilmente el dispositivo en tu lista de clientes.`,
  '/lan/ipv4': `## LAN – IPv4

### Bloque de red

El bloque de IP privadas /16 para tu red. Cada perfil de seguridad recibirá su propia subred /24 dentro de este bloque, permitiendo hasta 256 subredes separadas con 254 dispositivos cada una.

### IP del router

La IP LAN del router es la puerta de enlace (.1) de la subred del perfil de seguridad Admin. Los dos primeros octetos provienen del bloque de red anterior; el tercer octeto se establece en el campo Subred del perfil Admin, no en esta página.`,
  '/lan/ipv6': `## LAN – IPv6

### IPv6

Cuando está activado, los dispositivos de tu red recibirán direcciones IPv6 mediante SLAAC (Stateless Address Autoconfiguration). Es el método estándar para la asignación de direcciones IPv6.

IPv6 permite que tus dispositivos sean directamente accesibles desde internet sin NAT, lo cual es útil para alojar servicios o aplicaciones entre pares.

### Longitud de prefijo

La longitud de prefijo determina el tamaño del espacio de direcciones IPv6 de tu LAN. Debe ser **mayor** (un número más alto) que tu prefijo WAN para crear una subred válida.

Por ejemplo, si tu ISP te asigna un prefijo /48, puedes usar /56, /60 o /64 para tu LAN. Se recomienda un prefijo /64 para la mayoría de las redes domésticas.`,
  '/outbound': `## VPN salientes

Las VPN salientes enrutan tu tráfico de internet a través de túneles cifrados hacia servidores remotos, mejorando la privacidad y la seguridad.

### ¿Por qué usar VPN salientes?

-   Oculta tu dirección IP a sitios web y servicios
-   Cifra el tráfico en redes no fiables
-   Accede a contenido restringido geográficamente
-   Evita el rastreo por parte del ISP

### Encadenamiento de VPN

Enruta una VPN a través de otra para mayor privacidad. Tu tráfico sale a través de varios servidores, lo que dificulta rastrearlo hasta ti. Ten en cuenta que el encadenamiento aumenta la latencia.

### Primeros pasos

Sube un archivo de configuración de WireGuard de tu proveedor de VPN (Mullvad, ProtonVPN, IVPN, etc.) para crear un nuevo cliente VPN.`,
  '/outbound/vpn': `## VPN salientes

<details><summary>Etiqueta</summary> Un nombre amigable para identificar esta conexión VPN. Usa algo descriptivo como «Mullvad Suecia» o «VPN del trabajo». </details> <details><summary>Se conecta a</summary> Hacia dónde debe enrutarse el tráfico de esta VPN:<ul><li><b>Internet:</b> El tráfico sale directamente a internet a través de esta VPN.</li><li><b>Otra VPN:</b> Encadena primero a través de otra VPN para mayor privacidad. Tu tráfico se cifrará varias veces y saldrá a través de varios servidores.</li></ul><b>Nota:</b> Encadenar VPN aumenta la latencia y puede reducir las velocidades. </details> <details><summary>Ruta de conexión</summary> Muestra la ruta completa que sigue tu tráfico desde esta VPN hasta internet. Por ejemplo: Mullvad → Proton → Internet significa que el tráfico es cifrado por Mullvad, enviado a través de Proton y luego sale a internet.</details>`,
  '/outbound/dialog': `## VPN salientes

Los clientes VPN crean una conexión saliente a un servidor VPN, permitiendo a los dispositivos enviar y recibir datos como si estuvieran conectados directamente a la red privada. Enmascara tu dirección IP, sortea restricciones geográficas y protege los datos sensibles en redes Wi-Fi públicas.

### Etiqueta

Un nombre claro y descriptivo para la conexión VPN, como «Mullvad Suecia» o «VPN del trabajo», para diferenciarla fácilmente de otras conexiones.

### Archivo de configuración

Sube un archivo **.conf** de WireGuard de tu proveedor de VPN. Este archivo contiene la dirección del servidor, las claves y los ajustes de conexión necesarios para establecer el túnel. La mayoría de los proveedores (Mullvad, ProtonVPN, IVPN, etc.) ofrecen descargas de archivos de configuración de WireGuard desde su sitio web o aplicación.

### Destino

El siguiente salto en la ruta de una conexión VPN en capas.`,
  '/profiles': `## Perfiles

Los perfiles de seguridad definen segmentos de red con permisos aislados. Cada perfil controla el acceso a internet y la comunicación con otros perfiles de tu red local.

### Nombre

Un nombre descriptivo para el perfil de seguridad, como «Invitados» o «Dispositivos IoT».

### DNS

Qué servidores DNS usan los dispositivos de este perfil: los del sistema («Sistema»), los servidores personalizados del propio perfil («Personalizado») o, cuando se enruta a través de una VPN, los de la VPN («VPN»).

### Saliente

Cómo alcanza internet el tráfico de este perfil: directamente («Directo») o a través de uno de tus clientes VPN (mostrado por su nombre).

### Acceso LAN

Con qué otros perfiles puede comunicarse este perfil en la red local: con todos, solo con el suyo propio, o con una lista blanca elegida.

### Acceso WAN

Si los dispositivos de este perfil tienen acceso a internet, opcionalmente restringido por una lista blanca o lista negra de direcciones IP o rangos CIDR.`,
  '/profiles/dialog': `## Perfiles

Los perfiles de seguridad definen segmentos de red con permisos aislados. Cada perfil controla el acceso a internet y la comunicación con otros perfiles de tu red local.

### Nombre

Un nombre descriptivo para el perfil de seguridad, como «Invitados» o «Dispositivos IoT».

### Subred

El tercer octeto de la subred IPv4 para este perfil. Por ejemplo, un valor de 2 crea la subred 192.168.2.0/24. Cada perfil debe tener una subred única.

### Acceso LAN

Controla con qué otros perfiles puede comunicarse este perfil en la red local. Elige «Todos» para acceso completo, «Mismo perfil» para aislamiento, o «Lista blanca» para seleccionar perfiles individuales.

### Incluir automáticamente los nuevos perfiles en la lista blanca

Cuando el acceso LAN está configurado como «Lista blanca», activar esta opción concede también acceso a cualquier perfil creado en el futuro: útil para perfiles de administrador que deben alcanzar todos los segmentos de la red.

### Enrutamiento saliente

Elige cómo alcanza internet el tráfico de este perfil. Selecciona «Directo» para acceso normal a internet, o «VPN» para enrutar todo el tráfico del perfil a través de uno de tus clientes VPN.

### Acceso WAN

Si los dispositivos de este perfil tienen acceso a internet. Usa «Lista blanca» o «Lista negra» para restringir el acceso a direcciones IP o rangos CIDR específicos, o «Ninguno» para bloquear internet por completo.

### Programación de apagados

Horarios designados en los que se bloquea el acceso WAN (internet) para los dispositivos de este perfil. Fuera de estas franjas se aplican las reglas normales de acceso WAN del perfil. Por ejemplo, un perfil «Niños» podría bloquear el acceso WAN a las 19:00 en noches de colegio y a las 21:00 los fines de semana, mientras que un perfil «Admin» conserva acceso completo 24/7.

Esta sección se desactiva cuando el acceso WAN está configurado como «Ninguno»: ya que internet está bloqueado en todo momento, no hay nada que programar. Las franjas que hayas configurado se conservan y vuelven a aplicarse si reactivas el acceso WAN.

### DNS

Elige si este perfil usa los servidores DNS del sistema o los suyos propios. Selecciona «Personalizado» para especificar hasta tres servidores DNS, cada uno con cifrado DoH (DNS over HTTPS) opcional para búsquedas seguras.`,
  '/profiles/blackout': `## Perfiles – Apagados

Un apagado bloquea el acceso WAN (internet) para todos los dispositivos de este perfil durante el periodo de tiempo especificado. Las reglas normales de acceso WAN del perfil se aplican fuera de los apagados.

### Franja horaria

Las horas de inicio y fin del periodo de apagado. El acceso WAN se bloqueará entre estas horas en los días seleccionados. La hora de fin debe ser posterior a la hora de inicio.

### Días

Selecciona qué días de la semana debe estar activo el apagado.`,
  '/profiles/schedule': `### Programación de apagados

Horarios designados en los que se bloquea el acceso WAN (internet) para los dispositivos de este perfil. Fuera de estas franjas se aplican las reglas normales de acceso WAN del perfil. Por ejemplo, un perfil «Niños» podría bloquear el acceso WAN a las 19:00 en noches de colegio y a las 21:00 los fines de semana, mientras que un perfil «Admin» conserva acceso completo 24/7.

Haz clic en + para añadir una franja, haz doble clic en una franja existente para editarla o eliminarla, o arrastra sus controladores para redimensionarla en el sitio.`,
  '/published-ports': `## Puertos publicados

### Puertos publicados

Haz que determinados puertos de un dispositivo sean accesibles desde internet. El tráfico procedente de internet en el puerto especificado se reenviará al dispositivo seleccionado.

### Etiqueta

Un nombre descriptivo que te ayude a identificar esta regla, como «Home Assistant» o «Servidor de Minecraft».

### Dispositivo

Al dispositivo que selecciones se le asignará una dirección IPv4 estable para garantizar que la regla de reenvío de puertos siempre alcance el dispositivo previsto. Las direcciones IPv6 suelen ser estables mediante SLAAC.

### Puerto

El puerto o rango de puertos del dispositivo que se va a exponer. Por ejemplo, «443» para un solo puerto o «27015-27030» para un rango.

### Protocolo

Elige TCP para la mayoría de los servicios (web, SSH, etc.), UDP para juegos o VoIP, o ambos si el servicio lo requiere.

### Origen

Limita quién puede conectarse especificando una dirección IP o un rango CIDR. Usa «Cualquiera» para permitir conexiones desde cualquier lugar de internet.

### Endpoints

Las direcciones públicas donde se puede alcanzar este puerto. Los endpoints IPv4 usan NAT para reenviar el tráfico, mientras que IPv6 abre el firewall directamente al dispositivo.`,
  '/published-ports/dialog': `## Puertos publicados

Haz que determinados puertos de un dispositivo sean accesibles desde internet. El tráfico procedente de internet en el puerto especificado se reenviará al dispositivo seleccionado.

### Etiqueta

Un nombre descriptivo que te ayude a identificar esta regla, como «Home Assistant» o «Servidor de Minecraft».

### Dispositivo

El dispositivo al que reenviar el tráfico. Al dispositivo seleccionado se le asignará una dirección IPv4 estable para garantizar que la regla siempre alcance el dispositivo previsto.

### Puerto

El puerto o rango de puertos del dispositivo que se va a exponer. Por ejemplo, «443» para un solo puerto o «27015-27030» para un rango.

### Protocolo

Elige TCP para la mayoría de los servicios (web, SSH, etc.), UDP para juegos o VoIP, o ambos si el servicio lo requiere.

### Origen

Limita quién puede conectarse especificando una dirección IP o un rango CIDR. Usa «Cualquiera» para permitir conexiones desde cualquier lugar de internet.

### Versión de IP

Elige en qué versiones de IP publicar. IPv4 usa NAT para reenviar el tráfico, mientras que IPv6 abre el firewall directamente al dispositivo.

### Puerto externo

El número de puerto visible para el mundo exterior. Usa «Igual que el dispositivo» para mantenerlo idéntico, o especifica un puerto distinto para el acceso externo.`,
  '/wan/ipv4': `## WAN – IPv4

### Dirección IP

-   **DHCP:** Tu ISP asigna automáticamente una dirección IP a tu router. Es la configuración más habitual.
-   **PPPoE:** Usado por algunos proveedores de DSL. Requiere un nombre de usuario y una contraseña de tu ISP.
-   **Static:** Configura manualmente una dirección IP fija. Úsalo solo si tu ISP te ha asignado una IP estática.`,
  '/wan/ipv6': `## WAN – IPv6

### Dirección IP

-   **SLAAC:** Configuración automática de IPv6. La opción más habitual si tu ISP admite IPv6.
-   **DHCPv6:** El ISP asigna la dirección IPv6 mediante DHCP. Úsalo si SLAAC no funciona con tu ISP.
-   **Static:** Configura manualmente una dirección IPv6 fija asignada por tu ISP.
-   **6RD:** Tuneliza IPv6 sobre una conexión IPv4. Requiere los detalles de configuración de tu ISP.
-   **Disabled:** Deshabilita IPv6 en la interfaz WAN.

### Prefijo IPv6

Para SLAAC y DHCPv6, el prefijo especifica la longitud de prefijo solicitada para la delegación de prefijos (p. ej., /48, /56, /64). Déjalo vacío para que tu ISP decida automáticamente.

### DNS

DNS (Domain Name System) traduce los nombres de dominio a direcciones IP.

-   **Obtener del ISP:** Usa los servidores DNS proporcionados automáticamente por tu ISP.
-   **Custom:** Especifica tus propios servidores DNS. Se admiten tanto direcciones IPv4 como IPv6.`,
  '/wan/mac-address': `## WAN – Dirección MAC

### Dirección MAC

La dirección MAC identifica tu router ante tu ISP. Algunos ISP vinculan el servicio a una dirección MAC específica.

-   **Router:** Usa la dirección MAC integrada del router.
-   **Custom:** Clona la dirección MAC de un dispositivo anterior. Útil si tu ISP ha vinculado el servicio a la dirección MAC de tu antiguo router o módem.`,
  '/wan/dns': `## WAN – DNS

### DNS

DNS (Domain Name System) traduce los nombres de dominio a direcciones IP.

-   **Obtener del ISP:** Usa los servidores DNS proporcionados automáticamente por tu ISP.
-   **Custom:** Especifica tus propios servidores DNS (p. ej., 1.1.1.1, 8.8.8.8).

### Seguro (DoH)

Habilita DNS over HTTPS (DoH) para consultas DNS cifradas. Esto mejora la privacidad evitando la interceptación de tu tráfico DNS.

**Nota:** No todos los servidores DNS admiten DoH. Algunos servidores habituales que sí lo hacen son Cloudflare (1.1.1.1), Google (8.8.8.8) y Quad9 (9.9.9.9).`,
  '/wan/dynamic-dns': `## WAN – DNS dinámico

### DNS dinámico

DDNS asigna tu dirección IP cambiante a un nombre de dominio fijo, permitiéndote acceder a tu red de forma remota sin conocer tu IP actual.

-   **Start9:** Servicio DDNS gratuito proporcionado por Start9. No requiere configuración adicional.
-   **Otros proveedores:** Requiere una cuenta con el proveedor. Introduce las credenciales y el nombre de host de tu proveedor de DDNS.`,
  '/settings/activity': `## Ajustes – Actividad

Consulta los intentos de inicio de sesión recientes y los eventos de seguridad. Vigila la actividad sospechosa y elimina las entradas antiguas según sea necesario.`,
  '/settings/advanced': `## Ajustes – Registros

Opciones avanzadas para usuarios expertos. LuCI proporciona acceso directo a la configuración de OpenWRT. Los diagnósticos de soporte ayudan a solucionar problemas. El restablecimiento de fábrica restaura los ajustes predeterminados.`,
  '/settings/backup': `## Ajustes – Copia de seguridad

### Crear copia de seguridad

Descarga una copia de seguridad de todos los ajustes del router, incluidos los perfiles de seguridad, la configuración de WiFi, los certificados SSL y las preferencias del sistema. Guarda el archivo de copia de seguridad en un lugar seguro.

### Restaurar copia de seguridad

Sube un archivo de copia de seguridad creado previamente para restaurar todos los ajustes. El router se reiniciará tras la restauración y se sobrescribirán todos los ajustes actuales.`,
  '/settings/general': `## Ajustes – General

<details><summary>Preferencias</summary> Preferencias generales para la interfaz de usuario y el comportamiento del router.<h3>Tema</h3>Elige entre tema oscuro o claro, o mantén la configuración de tu sistema aplicada a la interfaz.<h3>Idioma</h3>Configura el idioma preferido para la interfaz de usuario del router. Este ajuste afecta al idioma mostrado en todos los menús, opciones y demás elementos de texto dentro de la interfaz web del router.<h3>Zona horaria</h3>Establece la zona horaria local del router. Afecta a los eventos programados como los apagados de WAN y las programaciones de apagado de WiFi. </details> <details><summary>Acceso remoto</summary> Habilita el acceso remoto para gestionar los ajustes y configuraciones del router desde fuera de la red local.<ul><li><b>Cuando esté detrás de NAT</b> (Network Address Translation): Permite el acceso remoto solo cuando el router forma parte de una red privada que usa NAT. NAT se usa habitualmente en redes domésticas y de pequeñas empresas donde varios dispositivos comparten una única dirección IP pública.</li><li><b>Nunca:</b> Deshabilita por completo el acceso remoto. Adecuado para entornos donde la seguridad es primordial, como en redes muy sensibles o aisladas.</li><li><b>Siempre:</b> Habilita el acceso remoto en todo momento. Ideal para entornos donde es necesaria una gestión remota continua, como en una red empresarial donde los administradores de TI necesitan mantener acceso constante al router para supervisión y actualizaciones.</li></ul></details><details><summary>Seguridad</summary> Descarga tu certificado Root CA para confiar en las conexiones HTTPS desde dispositivos adicionales. Instalar este certificado permite a los navegadores y aplicaciones verificar la identidad del router sin advertencias de seguridad. </details> <details><summary>Actualizaciones</summary> Cuando haya una actualización disponible, aparecerá un aviso en la parte superior de esta página. Puedes consultar las notas de la versión antes de actualizar. El router experimentará una breve interrupción durante el proceso de actualización.</details>`,
  '/settings/logs': `## Ajustes – Registros

Los registros recopilan los eventos del sistema y ayudan a diagnosticar problemas. Útiles para la solución de problemas y las consultas de soporte.`,
  '/settings/password': `## Ajustes – Contraseña

Cambia la contraseña de administrador del router. Mejora la seguridad protegiendo los ajustes del router y evitando el acceso no autorizado. Establece una contraseña fuerte y única.`,
  '/settings/ssh-keys': `## Ajustes – Claves SSH

Las claves SSH permiten una autenticación segura y sin contraseña en tu router. Añade claves públicas para los usuarios que necesiten acceso SSH.`,
  '/settings/ssh-keys/dialog': `## Ajustes – Claves SSH

Las claves SSH permiten una autenticación segura y sin contraseña en tu router. Añade la clave pública de un usuario que necesite acceso SSH.

### Clave pública

Pega la cadena completa de la clave pública, incluyendo el prefijo del algoritmo y los datos de la clave. Los formatos admitidos incluyen ssh-ed25519, ssh-rsa y ecdsa-sha2-nistp256/384/521.`,
  '/wifi/blackout-schedule': `## Wi-Fi – Programación de apagados

Especifica horarios designados en los que el WiFi se deshabilitará, evitando que cualquier dispositivo se conecte a él. Esta función se usa a menudo para restringir el acceso a internet durante periodos específicos con diversos fines, como reducir distracciones, hacer cumplir rutinas de hora de dormir o reforzar la seguridad de la red en horas no laborables.

**Haz doble clic en los intervalos existentes para editarlos o eliminarlos**

Arrastra los intervalos existentes para editarlos en el sitio.`,
  '/wifi/blackout-schedule/dialog': `## Wi-Fi – Programación de apagados

Un apagado deshabilita el WiFi durante el periodo de tiempo especificado, evitando que cualquier dispositivo se conecte. Útil para reducir distracciones, hacer cumplir rutinas de hora de dormir o reforzar la seguridad de la red en horas no laborables.

### Franja horaria

Las horas de inicio y fin del periodo de apagado. El WiFi se deshabilitará entre estas horas en los días seleccionados. La hora de fin debe ser posterior a la hora de inicio.

### Días

Selecciona qué días de la semana debe estar activo el apagado.`,
  '/wifi/passwords': `## Wi-Fi – Contraseñas

Gestiona las contraseñas de las redes inalámbricas. Cada entrada representa una red Wi-Fi con su propia contraseña y perfil de seguridad que controla los permisos de acceso.

### Perfil de seguridad

Controla a qué puede acceder el dispositivo conectado en la red. Asigna un perfil de seguridad para limitar o conceder acceso a recursos específicos.`,
  '/wifi/passwords/dialog': `## Wi-Fi – Contraseñas

Cada entrada representa una red Wi-Fi con su propia contraseña y perfil de seguridad que controla los permisos de acceso.

### Etiqueta

Un nombre descriptivo para esta red Wi-Fi, como «Casa» o «Red de invitados». Es el nombre de red (SSID) que verán los dispositivos.

### Contraseña

La contraseña que usarán los dispositivos para conectarse. Debe tener al menos 8 caracteres. Usa el botón de generar para crear una contraseña aleatoria fuerte.

### Perfil de seguridad

Controla a qué puede acceder el dispositivo conectado en la red. Asigna un perfil de seguridad para limitar o conceder acceso a recursos específicos.`,
  '/wifi/settings': `## Wi-Fi – Ajustes

### Habilitar Wi-Fi

Enciende o apaga la radio inalámbrica. Cuando está deshabilitada, ningún dispositivo puede conectarse por Wi-Fi.

### SSID

El SSID (Service Set Identifier) es el nombre de tu red WiFi. Identifica tu red ante los dispositivos, permitiéndoles encontrarla y conectarse a ella.

### Difusión

Difundir el SSID hace que la red sea detectable por los dispositivos que buscan conexiones inalámbricas. Deshabilitar la difusión oculta la red de los usuarios ocasionales, añadiendo una capa de seguridad.

### Banda de frecuencia

La banda de frecuencia en la que opera tu WiFi. 2,4 GHz ofrece mejor alcance, mientras que 5 GHz ofrece mayores velocidades. Elige «Ambas» para habilitar el funcionamiento de doble banda.

### Difundir por separado

Cuando se usan ambas bandas de frecuencia, esta opción crea SSID separados para 2,4 GHz y 5 GHz (p. ej., «MiRed» y «MiRed-5G»). Útil si quieres controlar explícitamente a qué banda se conecta un dispositivo.

### Canales

Canales específicos dentro de cada banda de frecuencia. Selecciona «Auto» para que el router elija el mejor canal, o elige un canal específico para evitar interferencias de redes vecinas.`,
}

export default HELP_ES
