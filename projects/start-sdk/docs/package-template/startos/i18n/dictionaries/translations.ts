import { LangDict } from './default'

export default {
  es_ES: {
    0: '¡Iniciando {{name}}!',
    1: 'Interfaz web',
    2: 'La interfaz web está lista',
    3: 'La interfaz web no está lista',
  },
  de_DE: {
    0: 'Starte {{name}}!',
    1: 'Weboberfläche',
    2: 'Die Weboberfläche ist bereit',
    3: 'Die Weboberfläche ist nicht bereit',
  },
  pl_PL: {
    0: 'Uruchamianie {{name}}!',
    1: 'Interfejs webowy',
    2: 'Interfejs webowy jest gotowy',
    3: 'Interfejs webowy nie jest gotowy',
  },
  fr_FR: {
    0: 'Démarrage de {{name}} !',
    1: 'Interface web',
    2: "L'interface web est prête",
    3: "L'interface web n'est pas prête",
  },
} satisfies Record<string, LangDict>
