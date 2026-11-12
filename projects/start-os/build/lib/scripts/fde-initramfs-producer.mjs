import { join } from 'node:path'

import { replaceOnce, requireReviewedContext } from './fde-reviewed-source.mjs'

const streamNames = {
  early: 'early.cpio',
  uncompressed: 'uncompressed.cpio',
  main: 'main.cpio',
}

export function capturedStreams(directory) {
  return {
    prefixes: [
      join(directory, streamNames.early),
      join(directory, streamNames.uncompressed),
    ],
    mainCpio: join(directory, streamNames.main),
  }
}

export function requireNativeHookFunctions(bytes) {
  requireReviewedContext(
    bytes,
    '8329c405b5af7cbae6c7409de334b0663add53b97eb7f7d608750808d86b1c93',
    'hook-functions',
  )
}

/**
 * Adapts reviewed native ordering/packaging without installing a provider patch.
 * The caller owns stable providers, the generated tree and a private capture directory.
 * FDE_NATIVE_HOOKS is the private directory admitted by prepareNativeHooks.
 * CONFDIR and its additional hooks also belong to this private generation.
 * FDE_ADMIT_SOURCES admits original runtime/configuration sources and native hook-functions.
 * Helpers and firmware also need admission before execution.
 * FDE_PREPARE must audit/harden the tree and native early stream before success.
 * FDE_CAPTURE_DIR holds the captured streams, outside the generated tree.
 * Captures establish byte provenance, not authorization to sign an image.
 */
export function instrumentProducer(mkinitramfs, hookFunctions) {
  requireReviewedContext(
    mkinitramfs,
    '16780cbe540fa1a9d7b1faad173b1fd74bdd4de3e56e773d10462d9ce77161bf',
    'mkinitramfs',
  )
  requireNativeHookFunctions(hookFunctions)
  let output = replaceOnce(
    mkinitramfs.toString(),
    'run_scripts /usr/share/initramfs-tools/hooks',
    ': "${FDE_NATIVE_HOOKS:?Missing admitted private native hooks}"\nrun_scripts "${FDE_NATIVE_HOOKS}"',
  )
  output = replaceOnce(
    output,
    '. /usr/share/initramfs-tools/scripts/functions',
    ': "${FDE_ADMIT_SOURCES:?Missing source admission}"\n"${FDE_ADMIT_SOURCES}" "${CONFDIR}" || exit 1\n\n. /usr/share/initramfs-tools/scripts/functions',
  )
  const preparation = `: "\${FDE_PREPARE:?Missing FDE preparation program}"
: "\${FDE_CAPTURE_DIR:?Missing private FDE capture directory}"
case "\${compress%% *}" in
	gzip|pigz) ;;
	*) echo "E: FDE producer requires gzip" >&2; exit 1 ;;
esac
fde_tree="$(realpath -e -- "\${DESTDIR}")" || exit 1
FDE_CAPTURE_DIR="$(realpath -e -- "\${FDE_CAPTURE_DIR}")" || exit 1
case "\${FDE_CAPTURE_DIR}/" in
	"\${fde_tree}/"*) echo "E: FDE capture directory is inside generated tree" >&2; exit 1 ;;
esac
for fde_capture in ${Object.values(streamNames).join(' ')}; do
	(umask 077; set -C; : > "\${FDE_CAPTURE_DIR}/\${fde_capture}") || exit 1
done
"\${FDE_PREPARE}" "\${DESTDIR}" || exit 1
if [ -s "\${__TMPCPIOGZ}" ]; then
	echo "E: Unsupported appended initramfs stream" >&2
	exit 1
fi
cp -- "\${__TMPEARLYCPIO}" "\${FDE_CAPTURE_DIR}/${streamNames.early}" || exit 1
`
  const packaging =
    '[ "${verbose}" = y ] && echo "Building cpio ${outfile} initramfs"\n'
  output = replaceOnce(output, packaging, preparation + '\n' + packaging)
  const uncompressed =
    '\t\t\tcpio --quiet $cpio_owner_root $cpio_reproducible -o -H newc -D "${DESTDIR}" <"${__TMPUNCOMPRESSEDFILES}" ||\n\t\t\t\t{ echo "E: mkinitramfs failure uncompressed cpio $?" >&2; echo 1 >&3; exit; }'
  output = replaceOnce(
    output,
    uncompressed,
    `\t\t\t{
${uncompressed}
\t\t\t} | tee "\${FDE_CAPTURE_DIR}/${streamNames.uncompressed}" ||
\t\t\t\t{ echo "E: FDE uncompressed capture failed" >&2; echo 1 >&3; exit; }`,
  )
  output = replaceOnce(
    output,
    '\t\t} | $compress -c ||',
    `\t\t} | {
\t\t\ttee "\${FDE_CAPTURE_DIR}/${streamNames.main}" ||
\t\t\t\t{ echo "E: FDE main capture failed" >&2; echo 1 >&3; exit; }
\t\t} | $compress -c ||`,
  )
  return output
}
