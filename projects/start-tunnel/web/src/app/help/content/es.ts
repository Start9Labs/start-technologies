// prettier-ignore
/** Spanish help content (lazy-loaded on language switch). Key -> markdown. */
const HELP_ES: Record<string, string> = {
  '/subnets': `## Subredes

Una subred es una red privada aislada (una VLAN). De forma predeterminada es una <code>/24</code> (254 dispositivos), pero el rango es configurable: puedes hacerla más grande. Cada dispositivo que añadas pertenece a una subred, y los dispositivos de la misma subred pueden comunicarse entre sí. StartTunnel incluye una subred predeterminada, que es suficiente para la mayoría de las configuraciones.

### Nombre

Una etiqueta descriptiva para distinguir tus redes.

### Rango IPv4

El bloque de direcciones privadas del que obtienen dirección los dispositivos de esta subred: una <code>/24</code> de forma predeterminada, o mayor (p. ej. <code>10.59.7.0/24</code>). Fijo una vez creada la subred.

### DNS

Cómo resuelve la subred los nombres de dominio: los resolutores del proveedor de VPS, un dispositivo de la subred o tus propios servidores personalizados.

### IPv4 de WAN y prefijo IPv6

Desde qué dirección pública sale el tráfico de la subred, y su bloque IPv6 enrutado opcional.

<a href="https://start9.com/start-tunnel/subnets.html" target="_blank" rel="noreferrer">Más información →</a>`,

  '/subnets/add': `## Añadir o editar subred

Crea una red privada o modifica una existente. El rango de IP solo puede establecerse al crearla; todo lo demás puede editarse después.

### Nombre

Una etiqueta descriptiva para la subred (obligatorio).

### Rango de IP

El bloque de IP privado de esta subred, en formato CIDR: una <code>/24</code> de forma predeterminada (254 dispositivos), o un bloque mayor (p. ej. <code>/16</code>) para más; no se permite un rango menor que <code>/24</code>. Se rellena con una sugerencia libre. Solo se muestra al crear: el rango no puede cambiarse después.

### DNS

- **Predeterminado (proveedor de VPS)**: usa los resolutores que proporciona tu VPS (lo más sencillo).
- **Dispositivo**: dirige la subred a un dispositivo de ella que ejecute su propio resolutor.
- **Personalizado**: introduce hasta tres direcciones de servidores DNS.

Este es el resolutor ascendente de la subred, distinto de los registros DNS que StartTunnel sirve para tus propios nombres de host.

### IP de WAN

Desde cuál de las direcciones IPv4 públicas del VPS sale el tráfico saliente de esta subred. **Predeterminado del sistema** deja que StartTunnel elija (la dirección se muestra entre paréntesis). Solo importa si tu VPS tiene más de una IP pública.

### Prefijo IPv6

Un bloque IPv6 enrutado opcional (p. ej. <code>2001:db8:abcd::/64</code>) para que cada dispositivo tenga una dirección global estable. Déjalo en blanco para ninguno.

<a href="https://start9.com/start-tunnel/subnets.html#creating-a-subnet" target="_blank" rel="noreferrer">Más información →</a>`,

  '/devices': `## Dispositivos

Todos los dispositivos de tu túnel, divididos en dos tablas.

### Servidores

Dispositivos que alojan servicios a los que otros acceden (normalmente un servidor StartOS). Cuando lo permites, un servidor puede gestionar los registros DNS del túnel y publicar sus propios puertos automáticamente: los dos interruptores de cada fila.

### Clientes

Teléfonos, portátiles y otros pares que solo se conectan hacia fuera; no tienen capacidades de configuración de la puerta de enlace.

### Columnas

Nombre, subred, IPv4 de LAN, la dirección pública desde la que sale el tráfico (IPv4 de WAN) y la IPv6 de cada dispositivo. Usa el menú de una fila para editarlo, ver su configuración de WireGuard, cambiar su rol o quitarlo.

<a href="https://start9.com/start-tunnel/devices.html" target="_blank" rel="noreferrer">Más información →</a>`,

  '/devices/add': `## Añadir o editar dispositivo

Añade un dispositivo a una subred, o renombra y reubica uno existente. Que sea **servidor** o **cliente** lo determina el botón «Añadir» que hayas usado. Al guardar un dispositivo nuevo, su configuración de WireGuard se abre automáticamente.

### Nombre

Un nombre descriptivo para el dispositivo (obligatorio).

### Subred

La red privada en la que colocar el dispositivo. Se selecciona sola cuando solo existe una subred. Solo se muestra al añadir.

### IP de LAN

La dirección del dispositivo dentro de la subred. Se rellena con la siguiente dirección libre; aparece una vez elegida la subred (solo al añadir).

### IP de WAN

Desde qué dirección pública sale el tráfico saliente de este dispositivo. **Predeterminado de la subred** hereda el ajuste de la subred (se muestra entre paréntesis); o fija una dirección concreta.

### Permitir inyección de DNS / Permitir publicación automática

Solo para servidores. Permiten que este servidor gestione los registros DNS del túnel y publique sus propios puertos mediante PCP/UPnP. Activados de forma predeterminada: actívalos solo para dispositivos de confianza.

<a href="https://start9.com/start-tunnel/devices.html#adding-a-device" target="_blank" rel="noreferrer">Más información →</a>`,

  '/devices/config': `## Configuración del dispositivo

La configuración de WireGuard de este dispositivo: úsala para conectarlo al túnel. Se abre automáticamente tras añadir un dispositivo, y en cualquier momento con **Ver configuración**.

### Archivo

La configuración como texto. **Cópiala** al portapapeles o **descárgala** como archivo <code>start-tunnel.conf</code> para importarla en un cliente de WireGuard.

### QR

Un código QR de la misma configuración. Escanéalo con la app móvil de WireGuard para configurar un teléfono o tableta sin teclear nada.

Para un servidor StartOS, añádela en <code>Sistema › Puertas de enlace</code> en lugar de una app de WireGuard.

<a href="https://start9.com/start-tunnel/devices.html#adding-a-device" target="_blank" rel="noreferrer">Más información →</a>`,

  '/published-ports': `## Puertos publicados

Enruta el tráfico entrante desde una dirección y puerto públicos hacia un puerto de uno de tus servidores. Dos tablas:

### Manual

Puertos que publicas aquí: activa o desactiva cada uno, renómbralo o elimínalo.

### Automático

Puertos que un servidor publica por sí mismo automáticamente (PCP/UPnP). Son de solo lectura aquí; gestiónalos donde se configura la publicación automática en el dispositivo.

### Columnas

Etiqueta, el **servidor** de destino, un **nombre de host** TLS opcional (SNI), los puertos externo e interno, el protocolo (siempre TCP/UDP) y la **IP** pública en la que es accesible.

<a href="https://start9.com/start-tunnel/published-ports.html" target="_blank" rel="noreferrer">Más información →</a>`,

  '/published-ports/add': `## Añadir puerto publicado

Envía el tráfico desde un puerto público a un servidor y puerto interno elegidos.

### Etiqueta

Un nombre para identificar este puerto publicado (obligatorio).

### Puerto externo

El puerto público al que se conecta la gente. Con un rango, es donde empieza.

### Servidor

Qué servidor recibe el tráfico (los clientes no pueden). Su dirección pública pasa a ser automáticamente la IP pública del puerto publicado.

### Puerto interno

El puerto del servidor que recibe el tráfico.

### Número de puertos

Publica varios puertos consecutivos a la vez, contando a partir de los puertos externo e interno. Déjalo en 1 para un solo puerto. Un rango no puede usar un nombre de host SNI.

### Versión de IP

Accede al servicio por IPv4, IPv6 o ambos. IPv6 requiere que la subred del servidor tenga un prefijo IPv6.

### Nombre de host (opcional)

Un dominio TLS/SSL (SNI) para que varios nombres de host compartan un mismo puerto externo. Solo IPv4, y no disponible para rangos.

<a href="https://start9.com/start-tunnel/published-ports.html#add-a-port-manually" target="_blank" rel="noreferrer">Más información →</a>`,

  '/published-ports/edit-label': `## Editar etiqueta

Renombra este puerto publicado, o añádele una etiqueta si aún no tiene. La etiqueta es solo para tu referencia en la lista: no afecta al enrutamiento.

<a href="https://start9.com/start-tunnel/published-ports.html#manual-and-automatic-ports" target="_blank" rel="noreferrer">Más información →</a>`,

  '/dns': `## Registros DNS

DNS privado para tu túnel. Estos registros permiten que los dispositivos del túnel accedan a los servicios que alojas mediante un nombre de host fácil de recordar (p. ej. <code>home.example.com</code>) en lugar de una IP del túnel, y solo se resuelven dentro de tu túnel, nunca en la red pública.

### Manual

Registros que añades a mano: apunta un nombre de host a uno de tus servidores (A/AAAA), crea un alias (CNAME) o guarda texto (TXT).

### Automático

Un servidor de confianza con la **inyección de DNS** activada registra aquí sus propios dominios de servicio automáticamente, para que no tengas que mantenerlos a mano. Son de solo lectura aquí: los gestionan los dispositivos.

Cada fila muestra el nombre de host, el tipo, el servidor de destino (o valor) y el TTL del registro.

<a href="https://start9.com/start-tunnel/dns-records.html" target="_blank" rel="noreferrer">Más información →</a>`,

  '/dns/add': `## Añadir registro DNS

Asigna un nombre de host a un dispositivo, o a un valor personalizado.

### Nombre de host

El nombre de host al que responde este registro, p. ej. <code>home.example.com</code> (obligatorio).

### Tipo

- **A / AAAA**: apunta un nombre a una dirección IPv4 / IPv6.
- **CNAME**: alias a otro nombre.
- **TXT**: texto arbitrario.

### Servidor / Valor

Para A y AAAA, elige uno de tus dispositivos servidor, o **Otro (personalizado)** para escribir una dirección a mano. Para CNAME y TXT, introduce el nombre o el texto de destino en **Valor**.

### TTL (segundos)

Cuánto tiempo pueden otros sistemas almacenar en caché este registro antes de volver a consultarlo. El valor predeterminado es 300.

<a href="https://start9.com/start-tunnel/dns-records.html#viewing-and-managing-records" target="_blank" rel="noreferrer">Más información →</a>`,

  '/settings': `## Ajustes

### Versión

La versión de StartTunnel instalada. **Buscar actualizaciones** comprueba si hay una versión más reciente; cuando la hay, **Actualizar a…** la descarga e instala.

### Redirección HTTP (80 → 443)

Para cada IPv4 pública, si los visitantes por <code>http://</code> simple se redirigen a <code>https://</code> seguro. Activado de forma predeterminada. Una dirección no puede tener a la vez una redirección y un puerto publicado en el puerto 80: el interruptor se desactiva mientras el puerto 80 esté publicado. Consulta <a href="https://start9.com/start-tunnel/http-redirects.html" target="_blank" rel="noreferrer">Redirecciones HTTP</a>.

### Idioma

El idioma en el que se muestra la interfaz.

### Cuenta

Cambia tu contraseña de acceso, reinicia el VPS o cierra sesión.

<a href="https://start9.com/start-tunnel/updating.html" target="_blank" rel="noreferrer">Más información →</a>`,

  '/settings/change-password': `## Cambiar contraseña

Establece una nueva contraseña para acceder a este VPS de StartTunnel.

### Nueva contraseña

Tu nueva contraseña: debe tener entre 8 y 64 caracteres.

### Confirmar nueva contraseña

Vuelve a escribirla exactamente; verás un error si las dos no coinciden.

Si alguna vez la olvidas, restablécela desde el VPS con <code>start-tunnel auth reset-password</code>.

<a href="https://start9.com/start-tunnel/faq.html#what-if-i-forget-my-password" target="_blank" rel="noreferrer">Más información →</a>`,
}

export default HELP_ES
