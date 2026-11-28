import { FdeEnrollmentProgress } from '../osBindings/rpc/start-os.schemas'

test('enrollment progress exposes firmware observations without a generation receipt', () => {
  const observation = {
    firmwareEnrolled: true,
    approvalCode: null,
    certificates: [],
  }

  expect(Object.keys(FdeEnrollmentProgress.shape).sort()).toEqual([
    'approvalCode',
    'certificates',
    'firmwareEnrolled',
  ])
  expect(FdeEnrollmentProgress.parse(observation)).toEqual(observation)
})
