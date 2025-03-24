# Format OCI et Format CRD

## Format OCI
Le format **OCI** est une norme utilisée pour stocker et partager des conteneurs et des artefacts logiciels comme des fichiers **WASM**, des exécutables ou d’autres composants.

### Comment ça marche ?
Imagine que tu veux envoyer un programme à un collègue. Plutôt que de lui donner plein de fichiers séparés, tu mets tout dans un **paquet standardisé** contenant :

- **Le programme lui-même** (exemple : un fichier `.wasm` ou un binaire).
- **Un fichier de description (manifest)** contenant des **métadonnées** comme la version, l’auteur, l’architecture, etc.

Ton collègue n’a plus qu’à extraire et exécuter le programme immédiatement.

### Exemple de manifest OCI
```json
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "config": {
    "mediaType": "application/vnd.oci.image.config.v1+json",
    "size": 7023,
    "digest": "sha256:abcdef123456..."
  },
  "layers": [
    {
      "mediaType": "application/wasm",
      "size": 12345,
      "digest": "sha256:123456abcdef..."
    }
  ],
  "annotations": {
    "org.opencontainers.image.title": "mon-programme",
    "org.opencontainers.image.version": "1.0.0"
  }
}
```

---

## Format CRD
Le format **CRD** permet de définir des **ressources personnalisées** dans un système informatique, notamment dans **Kubernetes**.

### Comment ça marche ?
Imagine que tu veux **créer un service spécifique** dans un système (exemple : un composant logiciel). Plutôt que d’écrire du code complexe, tu décris cette ressource dans un **fichier de configuration** au format YAML.

Ce fichier explique :
- **Le type de ressource** (ex: un composant, une base de données, une API, etc.).
- **Les paramètres et contraintes** (ex: quelle version, quelles options sont disponibles).
- **Comment le système doit gérer cette ressource**.

Une fois défini, le système peut **lire ce fichier et créer automatiquement la ressource**.

### Exemple de manifest CRD
```yaml
apiVersion: api.oam.dev/v1beta1
kind: Component
metadata:
  name: mon-composant
  annotations:
    description: "Un exemple de composant personnalisé"
    version: "1.0.0"
spec:
  type: service
  properties:
    image: "mon-image:latest"
    ports:
      - name: http
        port: 80
```

---

## Problème rencontré lors de la création du POC
Lors de la création de mon POC, j'ai remarqué :
- **Zot n'accepte que le format OCI (Open Container Initiative)** alors que le manifest envoyé est au format **CRD (Custom Resource Definition)**.

### Solutions actuelles
- Comme le format **n'est pas respecté** et que le **blob du config est souvent vide** (et est un fichier JSON dans la plupart des cas), j'envoie **le contenu du manifest au complet** à la place du config.
- Lors de ma **requête GET**, je récupère cette information et la stocke dans une variable **config**, placée à côté du **manifest OCI** et du fichier **WASM** en réponse.

### Rôle normal du fichier de configuration (config)
La configuration permet d’**identifier rapidement** :
- **Les importations et exportations** du composant.
- **Les mondes utilisés** par le composant.

En spécifiant ces éléments, on s’assure qu’un **runtime** peut valider que le composant possède toutes les couches nécessaires pour satisfaire les exportations. Cela garantit également qu’un **runtime WASM** peut **rejeter l’exécution** d’un composant s’il ne peut pas satisfaire les importations.

🔗 **Référence** : [WASM OCI Artifact](https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/#configmediatype-applicationvndwasmconfigv0json)

